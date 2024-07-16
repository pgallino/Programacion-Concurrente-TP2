use log::{error, info};
use std::{
    env,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use rand::Rng;
use tokio::task;

use crate::gateway::gateway_action::GatewayAction;
use crate::utils::messages::GatewayResponse;

pub struct PaymentGateway {
    socket: UdpSocket,
    pending_payments: Arc<Mutex<Vec<PaymentInformation>>>,
}

impl PaymentGateway {
    pub fn new(address: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(address)?;

        Ok(PaymentGateway {
            pending_payments: Arc::new(Mutex::new(Vec::new())),
            socket,
        })
    }

    pub fn start(&self) -> std::io::Result<()> {
        info!("[GATEWAY] Escuchando en {}", self.socket.local_addr()?);

        loop {
            let mut buf = [0; 1024];
            let (amt, src) = self.socket.recv_from(&mut buf)?;

            let msg = buf[..amt].to_vec();
            let clone = self.clone();
            let src_clone = src;

            task::spawn(async move {
                match GatewayAction::try_from(msg.clone()) {
                    Ok(action) => {
                        let result = match action {
                            GatewayAction::Capture {
                                order_id,
                                card_number,
                                amount,
                                owner_id,
                            } => clone
                                .capture(PaymentInformation {
                                    order_id,
                                    card_number,
                                    amount,
                                    owner_id,
                                })
                                .await
                                .map(|_| PaymentOk::Capture(CapturePaymentOk::Ok))
                                .map_err(PaymentError::Capture),
                            GatewayAction::Commit { order_id, owner_id } => clone
                                .commit(order_id, owner_id)
                                .await
                                .map(|_| PaymentOk::Commit(CommitPaymentOk::Ok))
                                .map_err(PaymentError::Commit),
                            GatewayAction::Abort { order_id, owner_id } => clone
                                .abort(order_id, owner_id)
                                .await
                                .map(|_| PaymentOk::Abort(AbortPaymentOk::Ok))
                                .map_err(PaymentError::Abort),
                        };

                        match result {
                            Ok(PaymentOk::Capture(CapturePaymentOk::Ok)) => {
                                let res = GatewayResponse::Acknowledge;
                                let res: Vec<u8> = res.into();

                                if let Err(e) = clone.socket.send_to(&res, src_clone) {
                                    error!("No se pudo enviar el acuse de recibo: {}", e);
                                }
                            }
                            Ok(PaymentOk::Commit(CommitPaymentOk::Ok)) => {
                                // No envío nada
                            }
                            Ok(PaymentOk::Abort(AbortPaymentOk::Ok)) => {
                                // no envío nada
                            }
                            Err(PaymentError::Capture(CapturePaymentError::RejectedCard)) => {
                                let res = GatewayResponse::RejectedCard;
                                let res: Vec<u8> = res.into();
                                if let Err(e) = clone.socket.send_to(&res, src_clone) {
                                    error!("No se pudo enviar el rechazo de tarjeta: {}", e);
                                }
                            }
                            Err(PaymentError::Capture(
                                CapturePaymentError::DuplicatedPendingOrder,
                            )) => {
                                let res = GatewayResponse::DuplicatedOrder;
                                let res: Vec<u8> = res.into();

                                if let Err(e) = clone.socket.send_to(&res, src_clone) {
                                    error!("No se pudo enviar el mensaje de duplicación: {}", e);
                                }
                            }
                            Err(PaymentError::Abort(AbortPaymentError::NoSuchPendingPayment)) => {}
                            Err(PaymentError::Commit(CommitPaymentError::NoSuchPendingPayment)) => {
                                let res = GatewayResponse::NoSuchPendingPayment;
                                let res: Vec<u8> = res.into();

                                if let Err(e) = clone.socket.send_to(&res, src_clone) {
                                    error!(
                                        "No se pudo enviar el mensaje de 'no existe tal pago': {}",
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                error!("No se pudo analizar el mensaje: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("No se pudo analizar el mensaje: {:?}", e);
                    }
                }
            });
        }
    }

    async fn capture(
        &self,
        info: PaymentInformation,
    ) -> Result<CapturePaymentOk, CapturePaymentError> {
        let mut pending_payments = self
            .pending_payments
            .lock()
            .map_err(|_| CapturePaymentError::MutexLockFailed)?;

        if pending_payments
            .iter()
            .any(|p| p.order_id == info.order_id && p.owner_id == info.owner_id)
        {
            return Err(CapturePaymentError::DuplicatedPendingOrder);
        }

        let probability: f64 = env::var("CARD_REJECTION_PROBABILITY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.3);
        let fail = rand::thread_rng().gen_bool(probability);

        let info_clone = info.clone();

        if fail {
            info!("[GATEWAY] Error al capturar pago de la orden: (SCREEN {} - ID {}), de monto $ {}. Razón: tarjeta {} rechazada.", info_clone.owner_id, info_clone.order_id, info_clone.amount, info_clone.card_number);
            return Err(CapturePaymentError::RejectedCard);
        }

        pending_payments.push(info);

        info!("[GATEWAY] Pago capturado correctamente (SCREEN {} - ID {}), de monto $ {}, a la tarjeta {}.", info_clone.owner_id, info_clone.order_id, info_clone.amount, info_clone.card_number);

        Ok(CapturePaymentOk::Ok)
    }

    async fn commit(
        &self,
        order_id: u8,
        owner_id: u8,
    ) -> Result<CommitPaymentOk, CommitPaymentError> {
        let mut pending_payments = self
            .pending_payments
            .lock()
            .map_err(|_| CommitPaymentError::MutexLockFailed)?;

        if !pending_payments
            .iter()
            .any(|p| p.order_id == order_id && p.owner_id == owner_id)
        {
            info!(
                "[GATEWAY] Error: no se pudo confirmar el pago (SCREEN {} - ID {}). Razón: no hay un pago pendiente asociado a la orden.",
                owner_id, order_id
            );
            return Err(CommitPaymentError::NoSuchPendingPayment);
        }

        pending_payments.retain(|p| p.order_id != order_id || p.owner_id != owner_id);

        info!(
            "[GATEWAY] Pago confirmado exitosamente (SCREEN {} - ID {}).",
            owner_id, order_id
        );

        Ok(CommitPaymentOk::Ok)
    }

    async fn abort(&self, order_id: u8, owner_id: u8) -> Result<AbortPaymentOk, AbortPaymentError> {
        let mut pending_payments = self
            .pending_payments
            .lock()
            .map_err(|_| AbortPaymentError::MutexLockFailed)?;

        if !pending_payments
            .iter()
            .any(|p| p.order_id == order_id && p.owner_id == owner_id)
        {
            info!(
                "[GATEWAY] Error: failed to abort payment (SCREEN {} - ID {}). Reason: no pending payment associated to the order.",
                owner_id, order_id
            );
            return Err(AbortPaymentError::NoSuchPendingPayment);
        }

        pending_payments.retain(|p| p.order_id != order_id || p.owner_id != owner_id);

        info!(
            "[GATEWAY] Succesfully aborted payment (SCREEN {} - ID {}).",
            owner_id, order_id
        );

        Ok(AbortPaymentOk::Ok)
    }
}

impl Clone for PaymentGateway {
    fn clone(&self) -> Self {
        PaymentGateway {
            socket: self.socket.try_clone().unwrap(),
            pending_payments: self.pending_payments.clone(),
        }
    }
}

#[derive(Debug)]
enum PaymentError {
    Capture(CapturePaymentError),
    Commit(CommitPaymentError),
    Abort(AbortPaymentError),
}

enum PaymentOk {
    Capture(CapturePaymentOk),
    Commit(CommitPaymentOk),
    Abort(AbortPaymentOk),
}

enum CapturePaymentOk {
    Ok,
}

enum CommitPaymentOk {
    Ok,
}

enum AbortPaymentOk {
    Ok,
}

#[derive(Debug)]
enum CapturePaymentError {
    RejectedCard,
    DuplicatedPendingOrder,
    MutexLockFailed,
}

#[derive(Debug)]
enum CommitPaymentError {
    NoSuchPendingPayment,
    MutexLockFailed,
}

#[derive(Debug)]
enum AbortPaymentError {
    NoSuchPendingPayment,
    MutexLockFailed,
}

#[derive(Debug, Clone)]
struct PaymentInformation {
    order_id: u8,
    card_number: u32,
    amount: f64,
    owner_id: u8,
}

// #[cfg(test)]
// mod tests {
//     // use super::*;

//     use std::{
//         env,
//         net::{IpAddr, Ipv4Addr, SocketAddr},
//     };

//     use serial_test::serial;

//     use crate::{
//         server::{CapturePaymentError, PaymentInformation},
//         PaymentGateway,
//     };

//     // Se usa el crate `serial_test` para garantizar que esta suite de tests se ejecuten en
//     // serie, para que la variable de entorno `CARD_REJECTION_PROBABILITY` no interfiera entre
//     // tests. Bajo algunos escenarios de testing la variable queda seteada de otro test
//     // corriendo concurremente y altera el resultado esperado.

//     #[tokio::test]
//     #[serial]
//     async fn test_can_capture_payment() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "0.0");

//         let info = PaymentInformation {
//             order_id: 1,
//             card_number: 1234,
//             amount: 100.0,
//         };

//         let cloned = info.clone();

//         let res = gateway.capture(info).await;
//         assert!(res.is_ok());

//         let some = gateway
//             .pending_payments
//             .clone()
//             .lock()
//             .expect("unable to lock payments")
//             .iter()
//             .any(|p| p.order_id == cloned.order_id);

//         assert!(some)
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_rejects_duplicated_order() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "1.0");

//         let first_order = PaymentInformation {
//             order_id: 1,
//             card_number: 1234,
//             amount: 100.0,
//         };

//         let duplicated = first_order.clone();

//         let _ = gateway.capture(first_order).await;

//         let res = gateway.capture(duplicated).await;
//         assert!(res.is_err())

//         // TODO: test error type is duplicated order
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_randomly_rejects_card_when_capturing() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "1.0");

//         let info = PaymentInformation {
//             order_id: 1,
//             card_number: 1234,
//             amount: 100.0,
//         };

//         let res = gateway.capture(info).await;
//         assert!(res.is_err())

//         // TODO: test error type is rejected card
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_can_commit_payment() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "0.0");

//         let info = PaymentInformation {
//             order_id: 1,
//             card_number: 1234,
//             amount: 100.0,
//         };

//         let cloned = info.clone();

//         let res_capture = gateway.capture(info).await;
//         assert!(res_capture.is_ok());

//         let some = gateway
//             .pending_payments
//             .clone()
//             .lock()
//             .expect("unable to lock payments")
//             .iter()
//             .any(|p| p.order_id == cloned.order_id);

//         assert!(some);

//         let res = gateway.commit(cloned.order_id).await;
//         assert!(res.is_ok());

//         let none = gateway
//             .pending_payments
//             .clone()
//             .lock()
//             .expect("unable to lock payments")
//             .iter()
//             .any(|p| p.order_id == cloned.order_id);

//         assert!(!none);
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_cannot_commit_unexistant_payment() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "0.0");

//         const NON_EXISTANT_ORDER: u32 = 5;
//         let res = gateway.commit(NON_EXISTANT_ORDER).await;
//         assert!(res.is_err())

//         // TODO: test error type is unexistant order
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_can_abort_payment() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "0.0");

//         let info = PaymentInformation {
//             order_id: 1,
//             card_number: 1234,
//             amount: 100.0,
//         };

//         let cloned = info.clone();

//         let res_capture = gateway.capture(info).await;
//         assert!(res_capture.is_ok());

//         let some = gateway
//             .pending_payments
//             .clone()
//             .lock()
//             .expect("unable to lock payments")
//             .iter()
//             .any(|p| p.order_id == cloned.order_id);

//         assert!(some);

//         let res = gateway.abort(cloned.order_id).await;
//         assert!(res.is_ok());

//         let none = gateway
//             .pending_payments
//             .clone()
//             .lock()
//             .expect("unable to lock payments")
//             .iter()
//             .any(|p| p.order_id == cloned.order_id);

//         assert!(!none);
//     }

//     #[tokio::test]
//     #[serial]
//     async fn test_cannot_abort_unexistant_payment() {
//         const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
//         let gateway = PaymentGateway::new(ADDRESS);

//         env::set_var("CARD_REJECTION_PROBABILITY", "0.0");

//         const NON_EXISTANT_ORDER: u32 = 5;
//         let res = gateway.abort(NON_EXISTANT_ORDER).await;
//         assert!(res.is_err())

//         // TODO: test error type is unexistant order
//     }
// }
