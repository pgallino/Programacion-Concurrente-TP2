use log::{debug, info};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    thread,
};

use crate::utils::{
    addresses::{id_to_addr_leader, id_to_addr_screen},
    errors::ScreenError,
    messages::{
        Alive, BullyElection, BullyOk, Coordinator, GatewayResponse, KeepAlive, Messages, Order,
        OrderJson, OrderResult, OrderStatus,
    },
};

use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Lines};
use std::time::Duration;

use crate::screen_lib::actors::{Inspect, LeaderReceiver, OrderCoordinator, OrderResolver};
use actix::prelude::*;
use ScreenStatus::*;

use crate::gateway::gateway_action::GatewayAction;

const CARDNUMBER: u32 = 32231244;
const PRICE: f64 = 1000.0;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ScreenStatus {
    ReadingOrder,
    WaitingGatewayResponse,
    WaitingRobotResponse,
    ChargingOrder,
    ElectingNewLeader,
    BeingLeader,
}

//====================// 🖥 SCREEN 🖥 //====================//
pub struct Screen {
    pub id: u8,
    pub status: ScreenStatus,
    pub socket: UdpSocket,
    pub leader: (Ipv4Addr, u16),
    pub peers: Vec<u8>,
    pub file_lines: Lines<BufReader<File>>,
    pub current_order: Option<Order>,
    pub nrobots: u8,
}

impl Screen {
    pub fn new(
        id: u8,
        peers: Vec<u8>,
        file_name: String,
        nrobots: u8,
    ) -> Result<Self, ScreenError> {
        let socket = Self::create_socket(id)?;
        let (leader, status) = Self::determine_leader(id, &peers);
        let file_lines = Self::open_file(file_name)?;
        let current_order = None;

        Ok(Screen {
            id,
            status,
            socket,
            leader,
            peers,
            file_lines,
            current_order,
            nrobots,
        })
    }

    fn create_socket(id: u8) -> Result<UdpSocket, ScreenError> {
        let socket = UdpSocket::bind(id_to_addr_screen(id as u16))?;
        info!("[Screen {}] conectada.", id);
        Ok(socket)
    }

    fn determine_leader(id: u8, peers: &[u8]) -> ((Ipv4Addr, u16), ScreenStatus) {
        let max_peers = peers.iter().max();
        let leader;
        if let Some(&leader_id) = max_peers {
            leader = id_to_addr_leader(leader_id as u16);
            if leader_id == id {
                return (leader, BeingLeader);
            };
        } else {
            leader = id_to_addr_leader(0);
        }
        (leader, ReadingOrder)
    }

    fn open_file(file_name: String) -> Result<io::Lines<io::BufReader<File>>, ScreenError> {
        let file = File::open(file_name)?;
        Ok(io::BufReader::new(file).lines())
    }

    // Inicializa el sistema de actores del Líder
    fn run_leader_logic(&self) -> Result<(), ScreenError> {
        info!("[Screen {}] Soy la Pantalla Líder.", self.id);

        let system = System::new();
        let id = self.id;
        system.block_on(async {
            let coordinator = OrderCoordinator::new(self.nrobots).start();

            // registro de los resolvers -> uno por cada robot.
            for i in 0..self.nrobots {
                let resolver = SyncArbiter::start(1, {
                    let addr = coordinator.clone();
                    move || OrderResolver::new(i, addr.clone())
                });
                resolver.try_send(Inspect {
                    resolver: resolver.clone(),
                })?;
            }

            SyncArbiter::start(1, {
                move || {
                    LeaderReceiver::new(
                        id,
                        coordinator.clone(),
                        UdpSocket::bind(id_to_addr_leader(id as u16)).unwrap(),
                    )
                }
            });

            for pid in self.peers.iter() {
                if *pid != self.id {
                    let coordinator_msg = Coordinator::new(self.id);
                    self.socket
                        .send_to(&coordinator_msg.as_bytes(), id_to_addr_screen(*pid as u16))?;
                }
            }

            Ok::<(), ScreenError>(())
        })?;

        system.run()?;
        Ok(())
    }

    pub fn leader_election(&mut self) -> Result<(), ScreenError> {
        for pid in self.peers.iter() {
            if *pid > self.id {
                let election = BullyElection { owner: self.id };
                self.socket
                    .send_to(&election.as_bytes(), id_to_addr_screen(*pid as u16))?;
                info!(
                    "[Screen {}] Continuo el proceso enviando ELECTION a {}",
                    self.id, *pid
                );
            }
        }
        self.socket
            .set_read_timeout(Some(Duration::from_secs(10)))?;

        let prev_status = self.status;
        self.status = ElectingNewLeader;
        self.listen_socket()?;

        if self.status != BeingLeader {
            self.status = prev_status;
        }

        if self.status == WaitingRobotResponse {
            if let Some(ref order) = self.current_order {
                self.socket.send_to(&order.as_bytes(), self.leader)?;
            }
        }
        Ok(())
    }

    // Handler de la election 🤝
    pub fn handle_election(&mut self, buffer: &[u8], from: SocketAddr) -> Result<(), ScreenError> {
        let owner = buffer[1];
        info!("[Screen {}] Llego un BullyElection de {}", self.id, owner);
        // como el que lo mandó es menor (id), le mando OK y continuo el proceso de elección
        let msg = BullyOk { owner: self.id };
        self.socket.send_to(&msg.as_bytes(), from)?;
        info!("[Screen {}] Le envié OK a {}", self.id, owner);
        // continuo el proceso mandando ELECTION a todos los de id mayor
        self.leader_election()?;
        Ok(())
    }

    pub fn handle_keepalive(&self, _buffer: &[u8], from: SocketAddr) -> Result<(), ScreenError> {
        self.socket.send_to(&Alive::new(self.id).as_bytes(), from)?;
        Ok(())
    }

    pub fn handle_coordinator(&mut self, buffer: &[u8]) {
        let leader = buffer[0];
        info!("[Screen {}] Tenemos nuevo lider! Es {}", self.id, leader);
        self.leader = id_to_addr_leader(leader as u16);
    }

    pub fn handle_orderresult(&mut self, buf: &[u8]) -> Result<(), ScreenError> {
        let order_result = OrderResult::from_bytes(buf)?;
        println!(
            "[Screen {}] Recibí el resultado del pedido: {:?}",
            self.id, order_result
        );

        // ======== PARTE DONDE SE COMUNICA CON EL GATEWAY ========
        let gateway_msg_bytes: Vec<u8> = if order_result.status == OrderStatus::Ready {
            let commit = GatewayAction::Commit {
                order_id: order_result.order_id,
                owner_id: order_result.owner_id,
            };
            commit.into()
        } else {
            let abort = GatewayAction::Abort {
                order_id: order_result.order_id,
                owner_id: order_result.owner_id,
            };
            abort.into()
        };

        self.socket
            .send_to(&gateway_msg_bytes, (Ipv4Addr::new(127, 0, 0, 1), 6000))?;

        info!("[Screen {}] Le envié al gateway el resultado", self.id);
        // ========================================================
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), ScreenError> {
        info!("[Screen {}] Arranque a funcionar", self.id);
        let mut order_id: u8 = 1;
        if self.status == BeingLeader {
            // Ejecuto la lógica del líder, esto es cuando es la primer pantalla lider
            self.run_leader_logic()?;
        } else {
            // espero a que me llegue el coordinator
            let mut buf = [0; 100];
            let (_, from) = self.socket.recv_from(&mut buf)?;
            let msg = Messages::try_from(buf[0])?;
            info!(
                "Me llego el {:?} de {:?}, arranco con los pedidos",
                msg, from
            );
            thread::sleep(Duration::from_secs(1));
            while let Some(line) = self.file_lines.next() {
                // Leer cada linea del archivo 📂 y procesar el pedido
                let line = line.expect("Error al leer una linea del archivo");

                // Parsear la linea como un objeto JSON
                let order: OrderJson =
                    serde_json::from_str(&line).expect("Error al deserializar la orden");

                self.current_order = Some(Order::new(self.id, order_id, order.items));
                let order_msg = match &self.current_order {
                    Some(order) => order,
                    None => continue,
                };
                println!("[Screen {}] Cree la orden {:?}", self.id, order_msg);
                let order_msg_clone = order_msg.clone();
                let order_bytes = order_msg.as_bytes();

                // ======== PARTE DONDE SE COMUNICA CON EL GATEWAY ========

                let prepare = GatewayAction::Capture {
                    order_id,
                    card_number: CARDNUMBER,
                    amount: PRICE,
                    owner_id: self.id,
                };
                let prepare_bytes: Vec<u8> = prepare.clone().into();

                self.socket
                    .send_to(&prepare_bytes, (Ipv4Addr::new(127, 0, 0, 1), 6000))?;
                self.status = WaitingGatewayResponse;

                info!(
                    "[Screen {}] Envíe Capture al Gateway {:?}",
                    self.id, prepare
                );
                self.listen_socket()?;

                // ========================================================

                // ======== PARTE DONDE SE LE ENVIA EL PEDIDO AL LIDER ========

                if self.status == WaitingRobotResponse {
                    println!(
                        "[Screen {}] El Gateway confirmó la captura del Pedido {}",
                        self.id, order_msg_clone.order_id
                    );
                    self.socket
                        .send_to(&order_bytes, self.leader)
                        .expect("Error al enviar el mensaje");
                    info!(
                        "[Screen {}] Pedido {} enviado a Leader",
                        self.id, order_msg_clone.order_id
                    );
                    debug!(
                        "[Screen {}] El pedido contiene {:?}",
                        self.id, order_msg_clone
                    );
                    self.socket
                        .set_read_timeout(Some(Duration::from_secs(10)))?;

                    // espero resultado:
                    println!(
                        "[Screen {}] Se encargó el pedido {}, esperando resultado...",
                        self.id, order_msg_clone.order_id
                    );
                    self.listen_socket()?;
                } else {
                    println!(
                        "[Screen {}] El Gateway rechazó la captura del Pedido {}",
                        self.id, order_msg_clone.order_id
                    );
                }

                if self.status == BeingLeader {
                    self.run_leader_logic()?;
                }
                // ============================================================
                order_id += 1;
            }
        }
        Ok(())
    }

    pub fn listen_socket(&mut self) -> Result<(), ScreenError> {
        let mut waiting_keepalive = false;
        loop {
            let mut buf = [0; 1024];
            let (size, from) = match self.socket.recv_from(&mut buf) {
                Ok(result) => result,
                Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                    // HUBO TIMEOUT
                    match &self.status {
                        WaitingRobotResponse => {
                            if waiting_keepalive {
                                info!("[Screen {}] El lider No me respondio el KEEPALIVE, arranco busqueda de nuevo lider", self.id);
                                self.leader_election()?;
                                if self.status == BeingLeader {
                                    break;
                                }
                                self.status = WaitingRobotResponse;
                                self.socket
                                    .set_read_timeout(Some(Duration::from_secs(10)))?;
                                waiting_keepalive = false;
                            } else {
                                info!(
                                    "[Screen {}] Tarda mucho el lider! Le mando un KEEPALIVE",
                                    self.id
                                );
                                waiting_keepalive = true;
                                self.socket
                                    .send_to(&KeepAlive::new(self.id).as_bytes(), self.leader)?;
                            };
                            continue;
                        }
                        ElectingNewLeader => {
                            // si soy el de ID mayor mando el mensaje a todos anunciando mi nuevo mandato
                            info!(
                                "[Screen {}] Soy el lider! Envio mensaje COORDINATOR al resto",
                                self.id
                            );
                            self.status = BeingLeader;
                            break;
                        }
                        _ => {}
                    }
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            // NO HUBO TIMEOUT
            if let Ok(msg) = Messages::try_from(buf[0]) {
                debug!(
                    "[Screen {}] Me llego un {:?} con el estado {:?}",
                    self.id, msg, self.status
                );
                match (msg, &self.status) {
                    // llega un mensaje de elección bully
                    (Messages::BullyElection, WaitingGatewayResponse) => {
                        self.handle_election(&buf[1..], from)?;
                    }
                    (Messages::BullyElection, WaitingRobotResponse) => {
                        self.handle_election(&buf[1..], from)?;
                        self.socket
                            .set_read_timeout(Some(Duration::from_secs(10)))?;
                        waiting_keepalive = false;
                        if self.status == BeingLeader {
                            break;
                        }
                    }
                    (Messages::BullyElection, ElectingNewLeader) => {
                        // me mandaron otro election, le mando ok y sigo con mi busqueda
                        let msg = BullyOk { owner: self.id };
                        self.socket.send_to(&msg.as_bytes(), from)?;
                    }
                    (Messages::BullyOk, ElectingNewLeader) => {
                        // hay uno mayor que yo, que continue con la busqueda, me quedo esperando al nuevo lider
                        self.socket.set_read_timeout(None)?;
                    }
                    // llega el mensaje de coordinador
                    (Messages::Coordinator, WaitingRobotResponse) => {
                        waiting_keepalive = false;
                        self.handle_coordinator(&buf[1..size]);
                        if let Some(ref order) = self.current_order {
                            debug!(
                                "[Screen {}] Le vuelvo a mandar el pedido a la lider",
                                self.id
                            );
                            self.socket.send_to(&order.as_bytes(), self.leader)?;
                        };
                    }
                    (Messages::Coordinator, ElectingNewLeader) => {
                        // me llego el mensaje de nuevo lider
                        self.handle_coordinator(&buf[1..size]);
                        break;
                    }
                    (Messages::Coordinator, _) => self.handle_coordinator(&buf[1..size]),
                    (Messages::GatewayResponse, WaitingGatewayResponse) => {
                        let result = GatewayResponse::try_from(buf[1])?;
                        match result {
                            GatewayResponse::Acknowledge => {
                                self.status = WaitingRobotResponse;
                            }
                            GatewayResponse::DuplicatedOrder => {
                                self.status = ReadingOrder;
                            }
                            GatewayResponse::NoSuchPendingPayment => {
                                self.status = ReadingOrder;
                            }
                            GatewayResponse::RejectedCard => {
                                self.status = ReadingOrder;
                            }
                        }
                        break;
                    }
                    (Messages::OrderResult, WaitingRobotResponse) => {
                        self.handle_orderresult(&buf[1..size])?;
                        break;
                    }
                    (Messages::OrderResult, ElectingNewLeader) => {
                        let result = OrderResult::from_bytes(&buf[1..size])?;
                        if result.status == OrderStatus::Ready {
                            self.status = WaitingRobotResponse;
                        } else {
                            self.status = ReadingOrder;
                        };
                        break;
                    }
                    (Messages::KeepAlive, _) => {
                        self.handle_keepalive(&buf[1..size], from)?;
                    }
                    (Messages::Alive, WaitingRobotResponse) => {
                        // le llega alive del lider, vuelve a esperar
                        waiting_keepalive = false;
                    }
                    (Messages::OrderRequest, _) => {
                        if let Some(order) = &self.current_order {
                            self.socket.send_to(&order.as_bytes(), from)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
