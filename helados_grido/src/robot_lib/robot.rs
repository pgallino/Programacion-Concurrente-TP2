use log::{debug, info};
use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex, RwLock,
    },
    thread,
    time::Duration,
};

use crate::{
    robot_lib::icecream::{Bucket, IceCream},
    utils::{
        addresses::{id_to_addr_resolver, id_to_addr_robot},
        errors::RobotError,
        messages::{
            Alive, Grade, Handshake, KeepAlive, Messages, Next, Order, OrderResult, OrderStatus,
            RobotDead, RobotOrder, RobotWithOrder, Token,
        },
    },
};

fn init_protocol(robot: &Robot) -> Result<(), RobotError> {
    let next_lock = robot.next.read()?;
    robot
        .socket
        .set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut ack_recv = false;
    let mut init_recv = false;
    loop {
        if !ack_recv {
            robot.socket.send_to(&[Messages::Init as u8], *next_lock)?;
        }
        let mut buf = [0; 100];
        match robot.socket.recv_from(&mut buf) {
            Ok(_) => {
                let msg = Messages::try_from(buf[0])?;
                if msg == Messages::Ack {
                    info!(
                        "[Screen {}] Me llego el ACK de mi siguiente, ya puedo continuar",
                        robot.id
                    );
                    ack_recv = true;
                    robot.socket.set_read_timeout(None)?;
                };
                if msg == Messages::Init {
                    info!(
                        "[Screen {}] Me llego el Init de mi anterior, le mando ACK",
                        robot.id
                    );
                    robot.socket.send_to(&[Messages::Ack as u8], robot.prev)?;
                    init_recv = true;
                }
            }
            Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                info!(
                    "[Screen {}] No me contestaron el Init, vuelvo a enviarlo",
                    robot.id
                );
            }
            Err(error) => return Err(error.into()),
        }
        if ack_recv && init_recv {
            robot
                .socket
                .set_read_timeout(Some(Duration::from_secs(10)))?;
            break;
        }
    }
    Ok(())
}

fn init_comunication(robot: &Robot) -> Result<(), RobotError> {
    let next_lock = robot.next.read()?;
    loop {
        let mut buf = [0; 100];
        match robot.socket.recv_from(&mut buf) {
            Ok(_) => {
                let msg = Messages::try_from(buf[0])?;
                if msg == Messages::Ack {
                    info!(
                        "[Screen {}] Me llego el ACK de mi siguiente, ya puedo continuar",
                        robot.id
                    );
                    break;
                };
                if msg == Messages::Init {
                    info!(
                        "[Screen {}] Me llego el Init de mi anterior, le mando ACK y reenvio el Init",
                        robot.id
                    );
                    robot.socket.send_to(&[Messages::Ack as u8], robot.prev)?;
                    robot.socket.send_to(&[Messages::Init as u8], *next_lock)?;
                    robot
                        .socket
                        .set_read_timeout(Some(Duration::from_secs(10)))?;
                }
            }
            Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                info!(
                    "[Screen {}] No me contestaron el Init, vuelvo a enviarlo",
                    robot.id
                );
                robot.socket.send_to(&[Messages::Init as u8], *next_lock)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn start_protocol(robot: &Robot) -> Result<(), RobotError> {
    if robot.id == 0 {
        init_protocol(robot)?;
        info!(
            "[RobotReceiver {}] Soy el minimo, envio los tokens",
            robot.id
        );
        let next_lock = robot.next.read()?;

        // ========= SE HACE EL ENVIO DE TOKENS INICIAL ========= //
        let chlte_token = Token {
            owner: robot.id,
            bucket: Bucket::new(IceCream::Chocolate, 20.0),
        };
        let vnla_token = Token {
            owner: robot.id,
            bucket: Bucket::new(IceCream::Vanilla, 20.0),
        };
        let ddl_token = Token {
            owner: robot.id,
            bucket: Bucket::new(IceCream::DulceDeLeche, 20.0),
        };
        let sby_token = Token {
            owner: robot.id,
            bucket: Bucket::new(IceCream::Strawberry, 20.0),
        };
        let lem_token = Token {
            owner: robot.id,
            bucket: Bucket::new(IceCream::Lemon, 20.0),
        };
        // ===================================================== //

        let tokens = [chlte_token, vnla_token, ddl_token, sby_token, lem_token];
        let mut token_status_lock = robot.token_status.lock()?;
        for token in tokens {
            if let Some((_, grade)) = token_status_lock.get_mut(&token.bucket.ice_cream) {
                *grade = grade.next();
            }
            robot.socket.send_to(&token.as_bytes(), *next_lock)?;
            thread::sleep(Duration::from_secs(1))
        }
    } else {
        init_comunication(robot)?;
    }
    Ok(())
}

pub struct OrderHandler {
    pub id: u8,
    pub socket: UdpSocket,
    pub next: Arc<RwLock<(Ipv4Addr, u16)>>,
    pub order: Arc<Mutex<Option<Order>>>,
    pub bucket_rx: Receiver<(Bucket, SocketAddr)>,
    pub token_status: Arc<Mutex<HashMap<IceCream, (f32, Grade)>>>,
    pub result_sent_pair: Arc<(Mutex<u8>, Condvar)>,
}

impl OrderHandler {
    pub fn new(
        id: u8,
        socket: UdpSocket,
        next: Arc<RwLock<(Ipv4Addr, u16)>>,
        order: Arc<Mutex<Option<Order>>>,
        bucket_rx: Receiver<(Bucket, SocketAddr)>,
        token_status: Arc<Mutex<HashMap<IceCream, (f32, Grade)>>>,
        result_sent_pair: Arc<(Mutex<u8>, Condvar)>,
    ) -> Self {
        OrderHandler {
            id,
            socket,
            next,
            order,
            bucket_rx,
            token_status,
            result_sent_pair,
        }
    }

    pub fn prepare(self) -> Result<(), RobotError> {
        loop {
            // Espera a que le llegue un token
            let (mut bucket, _from) = self.bucket_rx.recv()?;

            let mut order_lock = self.order.lock()?;
            let mut order_completed = false;
            let mut abort_order = false;
            let mut order_id: Option<u8> = None;
            let mut screen_id: Option<u8> = None;

            if let Some(ref mut order) = *order_lock {
                order_id = Some(order.order_id);
                screen_id = Some(order.screen_id);
                info!(
                    "[OrderHandler {}] Ya con el helado {:?} resuelvo el pedido {:?}",
                    self.id, bucket.ice_cream, order
                );
                if let Some(amount) = order.items.remove(&bucket.ice_cream) {
                    // Se fija si hay suficiente cantidad, sino cancela el pedido
                    abort_order = bucket.amount < amount;
                    if !abort_order {
                        let time = Duration::from_millis((amount * 1000.0) as u64);
                        info!(
                            "[OrderHandler {}] Voy a usar el helado, tardo {:?}",
                            self.id, time
                        );
                        bucket.amount -= amount;
                        thread::sleep(time);
                        order_completed = order.items.is_empty();
                    }
                }
            }

            drop(order_lock);

            if order_completed || abort_order {
                if let (Some(screen_id), Some(order_id)) = (screen_id, order_id) {
                    let order_result = if order_completed {
                        // ya cubrio todos los gustos del pedido
                        OrderResult::new(OrderStatus::Ready, order_id, screen_id)
                    } else {
                        info!(
                            "[OrderHandler {}] No tengo helado suficiente, aborto el pedido",
                            self.id
                        );
                        OrderResult::new(OrderStatus::Abort, order_id, screen_id)
                    };

                    let mut result_sent_lock = self.result_sent_pair.0.lock()?;
                    *result_sent_lock = 0;
                    drop(result_sent_lock);

                    loop {
                        info!(
                            "[OrderHandler {}] Envio el {:?} al OrderResolver {}",
                            self.id, order_result.status, self.id
                        );
                        self.socket.send_to(
                            &order_result.as_bytes(),
                            id_to_addr_resolver(self.id as u16),
                        )?;
                        let (result_sent, cvar) = &*self.result_sent_pair;
                        let mut result_sent_lock =
                            cvar.wait_while(result_sent.lock()?, |result_sent| *result_sent == 0)?;
                        if *result_sent_lock == 1 {
                            info!("[OrderHandler {}] Ya me confirmaron que el resultado se entrego exitosamente, me preparo para el proximo pedido", self.id);
                            *result_sent_lock = 0;
                            cvar.notify_one();
                            break;
                        }
                        *result_sent_lock = 0;
                        cvar.notify_one();
                    }
                }
                let mut order_lock = self.order.lock()?;
                *order_lock = None;
                drop(order_lock);
            }

            // Actualizo el token status
            let mut token_status_lock = self.token_status.lock()?;
            if let Some((amount, grade)) = token_status_lock.get_mut(&bucket.ice_cream) {
                *amount = bucket.amount;
                *grade = grade.next();
            }

            let token = Token {
                owner: self.id,
                bucket,
            };
            // Mando el token al siguiente
            self.socket
                .send_to(&token.clone().as_bytes(), *self.next.read()?)?;
        }
    }
}

pub struct RobotInspector {
    pub id: u8,
    pub ring_path: Vec<u8>,
    pub prev: (Ipv4Addr, u16),
    pub token_status: Vec<(IceCream, Grade)>,
}

impl RobotInspector {
    pub fn new(
        id: u8,
        ring_path: Vec<u8>,
        prev: (Ipv4Addr, u16),
        token_status: Vec<(IceCream, Grade)>,
    ) -> Self {
        RobotInspector {
            id,
            ring_path,
            prev,
            token_status,
        }
    }

    fn dead_robot_alert(self, socket: UdpSocket) -> Result<(), RobotError> {
        for (i, id) in self.ring_path.iter().enumerate() {
            if *id == self.id {
                let prev_to_prev_id =
                    self.ring_path[(i + self.ring_path.len() - 2) % self.ring_path.len()];
                socket.send_to(
                    &RobotDead::new(self.id, self.token_status).as_bytes(),
                    id_to_addr_robot(prev_to_prev_id as u16),
                )?;
                break;
            }
        }
        Ok(())
    }

    fn inspect_prev(self) -> Result<(), RobotError> {
        info!(
            "[RobotInspector {}] No recibi mensajes, envio un KEEPALIVE a mi anterior",
            self.id
        );
        let inspect_socket =
            UdpSocket::bind((Ipv4Addr::new(127, 0, 0, 1), 2100 + (self.id as u16)))?;
        inspect_socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        inspect_socket.send_to(&KeepAlive::new(self.id).as_bytes(), self.prev)?;

        let mut response = [0; 100];
        match inspect_socket.recv_from(&mut response) {
            Ok(_) => {
                debug!(
                    "[RobotInspector {}] Recibi respuesta, continuo esperando",
                    self.id
                );
                Ok(())
            }
            Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                info!(
                    "[RobotInspector {}] No recibi respuesta, mando alerta de DEADROBOT",
                    self.id
                );
                self.dead_robot_alert(inspect_socket)?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[derive()]
pub struct Robot {
    pub id: u8,
    pub ring_path: Vec<u8>,
    pub addr: (Ipv4Addr, u16),
    pub socket: UdpSocket,
    pub prev: (Ipv4Addr, u16),
    pub next: Arc<RwLock<(Ipv4Addr, u16)>>,
    pub order: Arc<Mutex<Option<Order>>>,
    pub token_status: Arc<Mutex<HashMap<IceCream, (f32, Grade)>>>,
    pub result_sent_pair: Arc<(Mutex<u8>, Condvar)>,
}

impl Robot {
    pub fn new(id: u8, ring_path: Vec<u8>, addr: (Ipv4Addr, u16)) -> Result<Self, RobotError> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(Duration::from_secs(10)))?;
        let prev = id_to_addr_robot(((id as usize + ring_path.len() - 1) % ring_path.len()) as u16); // agarra el anterior en el ring path
        let next = Arc::new(RwLock::new(id_to_addr_robot(
            ((id as usize + ring_path.len() + 1) % ring_path.len()) as u16,
        ))); // agarra el siguiente en el ring path

        let order = Arc::new(Mutex::new(None));
        let result_sent_pair = Arc::new((Mutex::new(0), Condvar::new()));
        let token_status = Arc::new(Mutex::new(
            [
                (IceCream::Vanilla, (20.0, Grade::A)),
                (IceCream::DulceDeLeche, (20.0, Grade::A)),
                (IceCream::Chocolate, (20.0, Grade::A)),
                (IceCream::Lemon, (20.0, Grade::A)),
                (IceCream::Strawberry, (20.0, Grade::A)),
            ]
            .iter()
            .cloned()
            .collect(),
        ));

        info!(
            "[Robot {}] Escucho en {:?}, con el path {:?}, mi anterior {:?} y mi siguiente {:?}",
            id, addr, ring_path, prev, next
        );

        Ok(Robot {
            id,
            ring_path,
            addr,
            socket,
            prev,
            next,
            order,
            token_status,
            result_sent_pair,
        })
    }

    pub fn start(&mut self) -> Result<(), RobotError> {
        info!("[Robot {}] Arranque a funcionar", self.id);

        let (bucket_tx, bucket_rx) = mpsc::channel::<(Bucket, SocketAddr)>();
        let order_handler = OrderHandler::new(
            self.id,
            self.socket.try_clone()?,
            self.next.clone(),
            self.order.clone(),
            bucket_rx,
            self.token_status.clone(),
            self.result_sent_pair.clone(),
        );

        start_protocol(self)?;

        // Arranca el OrderHandler
        let oh_handler = thread::spawn(move || order_handler.prepare());

        // Arranca el RobotReceiver
        self.receiver(bucket_tx)?;

        oh_handler.join()??;

        Ok(())
    }

    // ================== ROBOT RECEIVER ================== //

    fn receiver(&mut self, mut bucket_tx: Sender<(Bucket, SocketAddr)>) -> Result<(), RobotError> {
        info!("[RobotReceiver {}] Empece a escuchar", self.id);
        loop {
            let mut buf = [0; 100];
            let (_, from) = match self.socket.recv_from(&mut buf) {
                Ok(result) => result,
                Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                    let token_status = self
                        .token_status
                        .lock()?
                        .iter()
                        .map(|(ice_cream, (_, grade))| (*ice_cream, grade.clone()))
                        .collect();
                    let robot_inspector = RobotInspector::new(
                        self.id,
                        self.ring_path.clone(),
                        self.prev,
                        token_status,
                    );
                    thread::spawn(move || robot_inspector.inspect_prev());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let msg = Messages::try_from(buf[0])?;
            match msg {
                Messages::Handshake => self.handle_handshake(&buf)?,
                Messages::Token => self.handle_token(&buf, from, &mut bucket_tx)?,
                Messages::RobotOrder => self.handle_order(&buf)?,
                Messages::RobotDead => self.handle_robotdead(&buf)?,
                Messages::KeepAlive => self.handle_keepalive(&buf, from)?,
                Messages::KeepAliveFromResolver => {
                    self.handle_keepalive_from_resolver(&buf, from)?
                }
                Messages::NewLeader => self.handle_newleader(&buf, from)?,
                Messages::Ack => self.handle_ack()?,
                _ => {}
            }
        }
    }

    fn handle_handshake(&mut self, buffer: &[u8]) -> Result<(), RobotError> {
        let mut handshake = Handshake::from_bytes(&buffer[1..]);
        info!(
            "[RobotReceiver {}] Recibi un mensaje de handshake de {}, con la lista {:?}",
            self.id, handshake.owner, handshake.ids
        );
        let mut starting = 0;
        for (i, id) in self.ring_path.iter().enumerate() {
            if *id == handshake.ids[0] {
                starting = i;
                break;
            }
        }
        self.ring_path.splice(starting.., handshake.ids.clone());
        self.prev = id_to_addr_robot(self.ring_path[self.ring_path.len() - 1] as u16);
        info!(
            "[RobotReceiver {}] Nuevo ring path: {:?}, y prev: {:?}",
            self.id, self.ring_path, self.prev
        );

        if !handshake.ids.contains(&self.id) {
            // acomodar mi ring path
            handshake.ids.push(self.id);
            handshake.owner = self.id;
            info!(
                "[RobotReceiver {}] Envio mensaje handshake con la lista {:?}",
                self.id, handshake.ids
            );
            self.socket
                .send_to(&handshake.as_bytes(), *self.next.read()?)?;
        } else {
            info!(
                "[RobotReceiver {}] Ya dio la vuelta la lista que envie, listo mi handshake!",
                self.id
            );
        }
        Ok(())
    }

    fn handle_token(
        &mut self,
        buffer: &[u8],
        from: SocketAddr,
        bucket_tx: &mut Sender<(Bucket, SocketAddr)>,
    ) -> Result<(), RobotError> {
        let mut token = Token::from_bytes(&buffer[1..])?;

        token.owner = self.id;
        let order_lock = self.order.lock()?;
        if let Some(ref order) = *order_lock {
            if order.items.contains_key(&token.bucket.ice_cream) {
                bucket_tx.send((token.bucket, from))?;
                return Ok(());
            }
        }

        // Actualizo el token status
        let mut token_status_lock = self.token_status.lock()?;
        if let Some((amount, grade)) = token_status_lock.get_mut(&token.bucket.ice_cream) {
            *amount = token.bucket.amount;
            *grade = grade.next();
        }

        // Mando el token al siguiente
        self.socket.send_to(&token.as_bytes(), *self.next.read()?)?;
        Ok(())
    }

    fn handle_order(&mut self, buffer: &[u8]) -> Result<(), RobotError> {
        let order = RobotOrder::from_bytes(&buffer[1..]);
        info!("[RobotReceiver {}] Recibi el pedido {:?}, lo cargo para que lo maneje el OrderHandler.", self.id, order);
        let mut order_lock = self.order.lock()?;
        *order_lock = Some(order.order);
        Ok(())
    }

    fn handle_robotdead(&mut self, buffer: &[u8]) -> Result<(), RobotError> {
        info!("[RobotReceiver {}] Recibi un ROBOTDEAD", self.id);
        let msg = RobotDead::from_bytes(&buffer[1..]);
        let new_next = id_to_addr_robot(msg.owner as u16);
        let mut next_lock = self.next.write()?;
        *next_lock = new_next;

        self.socket.send_to(
            &Handshake::new(self.id, vec![self.id]).as_bytes(),
            *next_lock,
        )?;

        // envia los tokens que se perdieron en el camino
        let token_status_lock = self.token_status.lock()?;
        for (ice_cream, grade) in msg.status.iter() {
            if let Some((amount, my_grade)) = token_status_lock.get(ice_cream) {
                if (self.id < msg.owner && my_grade > grade)
                    || (self.id > msg.owner && my_grade == grade)
                {
                    let bucket = Bucket::new(*ice_cream, *amount);
                    info!(
                        "[RobotReceiver {}] Se perdio el token de {:?}, reenvio mi ultimo estado",
                        self.id, ice_cream
                    );
                    self.socket
                        .send_to(&Token::new(self.id, bucket).as_bytes(), *next_lock)?;
                };
            };
        }
        Ok(())
    }

    fn handle_keepalive(&mut self, _: &[u8], from: SocketAddr) -> Result<(), RobotError> {
        debug!("[RobotReceiver {}] Recibi un KEEPALIVE, respondo", self.id);
        self.socket.send_to(&Alive::new(self.id).as_bytes(), from)?;
        Ok(())
    }

    fn handle_keepalive_from_resolver(
        &mut self,
        _: &[u8],
        from: SocketAddr,
    ) -> Result<(), RobotError> {
        debug!(
            "[RobotReceiver {}] Recibi un KEEPALIVE de resolver, respondo",
            self.id
        );
        self.socket.send_to(&Alive::new(self.id).as_bytes(), from)?;
        Ok(())
    }

    fn handle_newleader(&mut self, _: &[u8], from: SocketAddr) -> Result<(), RobotError> {
        let order_lock = self.order.lock()?;
        if let Some(ref order) = *order_lock {
            info!(
                    "[RobotReceiver {}] Me llego un NEWLEADER, le mando que estoy atendiendo el pedido {:?} a la nueva pantalla lider",
                    self.id, order
                );
            self.socket.send_to(
                &RobotWithOrder::new(order.screen_id, order.order_id).as_bytes(),
                from,
            )?;
            let (result_sent, cvar) = &*self.result_sent_pair;
            let mut result_sent_lock = result_sent.lock()?;
            *result_sent_lock = 2;
            cvar.notify_one();
        } else {
            info!(
                    "[RobotReceiver {}] Me llego un NEWLEADER, le mando que estoy libre a la nueva pantalla lider",
                    self.id
                );
            self.socket
                .send_to(&[Messages::RobotAvailable as u8], from)?;
        }
        Ok(())
    }

    fn handle_ack(&mut self) -> Result<(), RobotError> {
        let (result_sent, cvar) = &*self.result_sent_pair;
        let mut result_sent_lock = result_sent.lock()?;
        *result_sent_lock = 1;
        cvar.notify_one();
        Ok(())
    }

    // ================== ROBOT RECEIVER ================== //
}
