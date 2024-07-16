use crate::robot_lib::icecream::IceCream;
use crate::utils::addresses::{id_to_addr_resolver, id_to_addr_robot, id_to_addr_screen};
use crate::utils::errors::ScreenError;
use crate::utils::messages::{
    Alive, KeepAliveFromResolver, Messages, NewLeader, Order, OrderResult, RobotOrder,
    RobotWithOrder,
};
use actix::prelude::*;
use log::{debug, error, info};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, UdpSocket};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum RobotStatus {
    Unknown,
    Available,
    Disavailable,
}

// ====================================== Actor Messages ====================================== //

#[derive(Message, Clone, Debug)]
#[rtype(result = "()")]
pub struct OrderActorMessage {
    pub screen_owner: u8,
    pub order_id: u8,
    pub items: HashMap<IceCream, f32>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ScreenError>")]
pub struct RegisterResolver {
    pub address: Addr<OrderResolver>,
    pub resolver_id: u8,
    pub robot_availability: bool,
    pub order: Option<(u8, u8)>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ScreenError>")]
struct RobotOrderActorMessage {
    order: OrderActorMessage,
    robot_id: u8,
}

#[derive(Message)]
#[rtype(result = "()")]
struct FallenRobotActorMessage {
    robot_id: u8,
    order: OrderActorMessage,
}

#[derive(Message)]
#[rtype(result = "()")]
struct FreeRobotActorMessage {
    robot_id: u8,
    order: (u8, u8),
}

#[derive(Message)]
#[rtype(result = "Result<(), ScreenError>")]
pub struct Inspect {
    pub resolver: Addr<OrderResolver>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ScreenError>")]
pub struct ListenRobot {
    order: (u8, u8),
}

// ====================================== LeaderReceiver ====================================== //

pub struct LeaderReceiver {
    id: u8,
    order_coordinator: Addr<OrderCoordinator>,
    socket: UdpSocket,
}

impl LeaderReceiver {
    pub fn new(id: u8, order_coordinator: Addr<OrderCoordinator>, socket: UdpSocket) -> Self {
        LeaderReceiver {
            id,
            order_coordinator,
            socket,
        }
    }

    fn receive_msgs(&self) -> Result<(), ScreenError> {
        loop {
            let mut buf = [0; 1024];
            let (len, from) = self.socket.recv_from(&mut buf)?;
            debug!("[LeaderReceiver] Recibí un mensaje de: {:?}", from);
            if let Ok(msg) = Messages::try_from(buf[0]) {
                match msg {
                    // llega un mensaje de Order
                    Messages::Order => {
                        let pedido_msg = Order::from_bytes(&buf[1..len]);
                        let pedido = OrderActorMessage {
                            screen_owner: pedido_msg.screen_id,
                            order_id: pedido_msg.order_id,
                            items: pedido_msg.items,
                        };
                        info!("[LeaderReceiver] Recibí un pedido: {:?}", pedido);
                        self.order_coordinator.do_send(pedido);
                    }
                    Messages::KeepAlive => {
                        self.socket.send_to(&Alive::new(self.id).as_bytes(), from)?;
                        debug!("[LeaderReceiver] Respondo KeepAlive");
                    }
                    _ => {}
                }
            }
        }
    }
}

impl Actor for LeaderReceiver {
    type Context = SyncContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        self.receive_msgs().unwrap();
    }
}

// ====================================== OrderCoordinator ====================================== //

#[derive(Debug)]
pub struct OrderCoordinator {
    fulfilled_orders: HashSet<(u8, u8)>,
    robot_states: HashMap<u8, (bool, Addr<OrderResolver>)>,
    pending_orders: VecDeque<OrderActorMessage>,
    nresolvers: u8,
}

impl OrderCoordinator {
    pub fn new(nresolvers: u8) -> Self {
        OrderCoordinator {
            fulfilled_orders: HashSet::new(),
            robot_states: HashMap::new(),
            pending_orders: VecDeque::new(),
            nresolvers,
        }
    }

    fn assign_order(&mut self, order: OrderActorMessage) {
        info!(
            "[Coordinator] Buscando resolver disponible para realizar el pedido: {:?}",
            order
        );

        if !self
            .fulfilled_orders
            .contains(&(order.screen_owner, order.order_id))
        {
            // Buscamos un resolver disponible
            if let Some((id, (available, resolver))) = self
                .robot_states
                .iter_mut()
                .find(|(_, (available, _))| *available)
            {
                // Clonamos la dirección del resolver para usarla más tarde
                let resolver_addr = resolver;

                // Marcamos al resolver como no disponible
                *available = false;

                info!("[Coordinator] Le delego el pedido al OrderResolver {}", *id);
                // Enviamos el mensaje al resolver del robot
                resolver_addr.do_send(RobotOrderActorMessage {
                    order,
                    robot_id: *id,
                });

                return;
            }
            info!("[Coordinator] No se encontró ningún resolver disponible. Poniendo en cola el pedido: {:?}", order);
            self.pending_orders.push_back(order);
        }
    }

    fn free_robot(&mut self, robot_id: u8) {
        if let Some(resolver) = self.robot_states.get(&robot_id) {
            let resolver_addr = resolver.1.clone();
            self.robot_states.insert(robot_id, (true, resolver_addr));
            if let Some(order) = self.pending_orders.pop_front() {
                self.assign_order(order);
            }
        }
    }

    fn handle_robot_failure(&mut self, robot_id: u8, order: OrderActorMessage) {
        self.robot_states.remove(&robot_id);
        self.fulfilled_orders
            .remove(&(order.screen_owner, order.order_id));
        self.assign_order(order);
    }
}

impl Actor for OrderCoordinator {
    type Context = Context<Self>;
}

impl Handler<RegisterResolver> for OrderCoordinator {
    type Result = Result<(), ScreenError>;

    fn handle(&mut self, msg: RegisterResolver, _: &mut Self::Context) -> Self::Result {
        info!("[Coordinator] registre al resolver: {}", msg.resolver_id);
        self.nresolvers -= 1;
        self.robot_states.insert(
            msg.resolver_id,
            (msg.robot_availability, msg.address.clone()),
        );
        if let Some(order) = msg.order {
            debug!("[Coordinator] Ya habia orden siendo resuelta");
            self.fulfilled_orders.insert(order);
            msg.address.try_send(ListenRobot { order })?;
        };
        if self.nresolvers == 0 {
            info!("[Coordinator] Ya no hay mas resolvers por registrarse, chequeo pedidos");
            let initial_pending_orders_amount = self.pending_orders.len();
            for _ in 0..initial_pending_orders_amount {
                if let Some(order) = self.pending_orders.pop_front() {
                    if !self
                        .fulfilled_orders
                        .contains(&(order.screen_owner, order.order_id))
                    {
                        self.pending_orders.push_back(order);
                    } else {
                        self.assign_order(order);
                    }
                }
            }
        }
        Ok(())
    }
}

impl Handler<OrderActorMessage> for OrderCoordinator {
    type Result = ();

    fn handle(&mut self, msg: OrderActorMessage, _ctx: &mut Self::Context) {
        info!("[Coordinator] Me llego el pedido {:?}", msg);
        if self.nresolvers == 0 {
            self.assign_order(msg);
        } else {
            info!("[Coordinator] Mando el pedido directamente en la cola");
            self.pending_orders.push_back(msg);
        }
    }
}

impl Handler<FallenRobotActorMessage> for OrderCoordinator {
    type Result = ();

    fn handle(&mut self, msg: FallenRobotActorMessage, _ctx: &mut Self::Context) {
        self.handle_robot_failure(msg.robot_id, msg.order);
    }
}

impl Handler<FreeRobotActorMessage> for OrderCoordinator {
    type Result = ();

    fn handle(&mut self, msg: FreeRobotActorMessage, _ctx: &mut Self::Context) {
        info!("[Coordinator] Robot {} disponible", msg.robot_id);
        self.fulfilled_orders.remove(&msg.order);
        self.free_robot(msg.robot_id);
    }
}

// ====================================== OrderResolver ====================================== //

#[derive(Debug)]
pub struct OrderResolver {
    id: u8,
    order_coordinator: Addr<OrderCoordinator>,
    socket: UdpSocket,
}

impl OrderResolver {
    pub fn new(id: u8, order_coordinator: Addr<OrderCoordinator>) -> Self {
        let socket = UdpSocket::bind(id_to_addr_resolver(id as u16)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(12)))
            .unwrap();
        OrderResolver {
            id,
            order_coordinator,
            socket,
        }
    }

    fn send_order_to_robot(
        &self,
        order: RobotOrder,
        robot_addr: (Ipv4Addr, u16),
    ) -> Result<(), ScreenError> {
        self.socket.send_to(&order.as_bytes(), robot_addr)?;
        info!(
            "[Resolver {}] Envié pedido ({}, {}) al robot {:?}",
            self.id, order.order.screen_id, order.order.order_id, robot_addr
        );
        Ok(())
    }

    // Recibe respuesta del Robot

    pub fn receive_order_result_ka(
        &self,
        robot_id: u8,
    ) -> Result<Option<OrderResult>, ScreenError> {
        debug!(
            "[Resolver {}] Esperando respuesta del robot {}:",
            self.id, robot_id
        );
        let robot_addr = id_to_addr_robot(robot_id as u16);

        let mut waiting_keepalive = false;
        let mut buf = [0; 1024];

        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((size, from)) => match buf[0] {
                    x if x == Messages::Alive as u8 => {
                        let alive = Alive::from_bytes(&buf[1..size]);
                        debug!(
                            "[Resolver {}] Alive recibido del robot {}:",
                            self.id, alive.owner
                        );
                        waiting_keepalive = false;
                    }
                    x if x == Messages::OrderResult as u8 => {
                        let order_result = OrderResult::from_bytes(&buf[1..size])?;
                        info!(
                                "[Resolver {}] OrderResult recibido del robot {}: {:?}, le envio el ACK al robot",
                                self.id, robot_id, order_result
                            );
                        self.socket.send_to(&[Messages::Ack as u8], from)?;
                        return Ok(Some(order_result));
                    }
                    _ => {
                        error!(
                            "[Resolver {}] mensaje desconocido recibido del robot {}: {:?}",
                            self.id,
                            robot_id,
                            &buf[..size]
                        );
                    }
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Timeout alcanzado sin recibir nada
                    if waiting_keepalive {
                        // No hay respuesta al keepalive, asumir que el robot está muerto
                        break;
                    } else {
                        // Enviar mensaje keep_alive al robot
                        let keep_alive = KeepAliveFromResolver::new(self.id);
                        let keep_alive_msg = keep_alive.as_bytes();
                        self.socket.send_to(&keep_alive_msg, robot_addr)?;
                        waiting_keepalive = true;
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        info!("[Resolver {}] El robot {} no respondió, asumiendo que está muerto y reenviando el pedido", self.id, robot_id);
        Ok(None)
    }

    fn send_result_to_screen(
        &self,
        order_result: OrderResult,
        screen_id: u8,
    ) -> Result<(), ScreenError> {
        let screen_addr = id_to_addr_screen(screen_id as u16);
        let result_bytes = order_result.as_bytes();

        self.socket.send_to(&result_bytes, screen_addr)?;
        info!(
            "[Resolver {}] Envíe resultado a la screen {:?}",
            self.id, screen_addr
        );
        Ok(())
    }

    fn communicate_with_robot(
        &self,
        robot_id: u8,
        order: OrderActorMessage,
    ) -> Result<bool, ScreenError> {
        let robot_addr = id_to_addr_robot(robot_id as u16);
        let order_msg = RobotOrder::new(self.id, order.screen_owner, order.order_id, order.items);

        self.send_order_to_robot(order_msg, robot_addr)?;

        if let Some(order_result) = self.receive_order_result_ka(robot_id)? {
            self.send_result_to_screen(order_result, order.screen_owner)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Actor for OrderResolver {
    type Context = SyncContext<Self>;
}

impl Handler<RobotOrderActorMessage> for OrderResolver {
    type Result = Result<(), ScreenError>;

    fn handle(
        &mut self,
        robot_order: RobotOrderActorMessage,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        debug!(
            "[Resolver {}] Recibi el pedido {:?}",
            self.id, robot_order.order
        );
        let order = robot_order.order;
        let robot_id = robot_order.robot_id;
        if self.communicate_with_robot(robot_id, order.clone())? {
            info!(
                "[Resolver {}] comunico al coordinator que el robot {} termino el pedido {:?}",
                self.id, robot_id, order
            );
            self.order_coordinator.do_send(FreeRobotActorMessage {
                robot_id,
                order: (order.screen_owner, order.order_id),
            });
        } else {
            info!("[Resolver {}] comunico al coordinator que el robot {} esta caido y la orden perdida {:?}", self.id, robot_id, order);
            self.order_coordinator
                .do_send(FallenRobotActorMessage { robot_id, order });
            ctx.stop();
        }
        Ok(())
    }
}

impl Handler<Inspect> for OrderResolver {
    type Result = Result<(), ScreenError>;

    fn handle(&mut self, robot_order: Inspect, ctx: &mut Self::Context) -> Self::Result {
        self.socket.send_to(
            &NewLeader::new(self.id).as_bytes(),
            id_to_addr_robot(self.id as u16),
        )?;
        let mut buf = [0; 1024];
        let (size, _) = match self.socket.recv_from(&mut buf) {
            Ok(result) => result,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                info!(
                    "[Resolver {}] Le mando al Coordinator mi registro con robot caido",
                    self.id
                );
                self.order_coordinator.try_send(RegisterResolver {
                    address: robot_order.resolver,
                    resolver_id: self.id,
                    robot_availability: false,
                    order: None,
                })?;
                ctx.stop();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        if let Ok(msg) = Messages::try_from(buf[0]) {
            if msg == Messages::RobotWithOrder {
                let order = RobotWithOrder::from_bytes(&buf[1..size]);
                info!(
                    "[Resolver {}] Le mando al Coordinator mi registro con robot trabajando",
                    self.id
                );
                self.order_coordinator.try_send(RegisterResolver {
                    address: robot_order.resolver,
                    resolver_id: self.id,
                    robot_availability: false,
                    order: Some((order.screen_id, order.order_id)),
                })?;
            } else {
                info!(
                    "[Resolver {}] Le mando al Coordinator mi registro con robot libre",
                    self.id
                );
                self.order_coordinator.try_send(RegisterResolver {
                    address: robot_order.resolver,
                    resolver_id: self.id,
                    robot_availability: true,
                    order: None,
                })?;
            };
        };
        Ok(())
    }
}

impl Handler<ListenRobot> for OrderResolver {
    type Result = Result<(), ScreenError>;

    fn handle(&mut self, msg: ListenRobot, _ctx: &mut Self::Context) -> Self::Result {
        debug!("[Resolver {}] Me llego un  ListenRobot", self.id);
        if let Some(order_result) = self.receive_order_result_ka(self.id)? {
            self.send_result_to_screen(order_result, msg.order.0)?;
            debug!(
                "[Resolver {}] comunico al coordinator que el robot {} terminó el pedido {:?}",
                self.id, self.id, msg.order
            );
            self.order_coordinator.do_send(FreeRobotActorMessage {
                robot_id: self.id,
                order: msg.order,
            });
        };
        Ok(())
    }
}
