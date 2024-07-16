use crate::robot_lib::icecream::{Bucket, IceCream};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashMap};

use super::errors::ParseError;

#[derive(Clone, Debug, PartialEq)]
pub enum Grade {
    A = 0,
    B = 1,
    C = 2,
}

pub trait Next {
    fn next(&self) -> Grade;
}

impl Next for Grade {
    fn next(&self) -> Grade {
        match self {
            Grade::A => Grade::B,
            Grade::B => Grade::C,
            Grade::C => Grade::A,
        }
    }
}

impl PartialOrd for Grade {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use Grade::*;
        match (self, other) {
            (B, A) | (C, B) | (A, C) => Some(Ordering::Greater),
            (A, B) | (B, C) | (C, A) => Some(Ordering::Less),
            _ => Some(Ordering::Equal),
        }
    }
}

impl TryFrom<u8> for Grade {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Grade::A),
            1 => Ok(Grade::B),
            2 => Ok(Grade::C),
            _ => Err(ParseError::ConversionError),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Messages {
    Handshake = 0,
    Token = 1,
    Ack = 2,
    Order = 3,
    KeepAlive = 4,
    BullyElection = 5,
    BullyOk = 6,
    Coordinator = 7,
    OrderResult = 8,
    Alive = 9,
    NewLeader = 10,
    RobotDead = 11,
    KeepAliveFromResolver = 12,
    RobotOrder = 13,
    Prepare = 14,
    OrderRequest = 15,
    RobotAvailable = 16,
    RobotWithOrder = 17,
    GatewayResponse = 18,
    Init = 19,
}

impl TryFrom<u8> for Messages {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Messages::Handshake),
            1 => Ok(Messages::Token),
            2 => Ok(Messages::Ack),
            3 => Ok(Messages::Order),
            4 => Ok(Messages::KeepAlive),
            5 => Ok(Messages::BullyElection),
            6 => Ok(Messages::BullyOk),
            7 => Ok(Messages::Coordinator),
            8 => Ok(Messages::OrderResult),
            9 => Ok(Messages::Alive),
            10 => Ok(Messages::NewLeader),
            11 => Ok(Messages::RobotDead),
            12 => Ok(Messages::KeepAliveFromResolver),
            13 => Ok(Messages::RobotOrder),
            14 => Ok(Messages::Prepare),
            15 => Ok(Messages::OrderRequest),
            16 => Ok(Messages::RobotAvailable),
            17 => Ok(Messages::RobotWithOrder),
            18 => Ok(Messages::GatewayResponse),
            19 => Ok(Messages::Init),
            _ => Err(ParseError::ConversionError),
        }
    }
}

#[derive()]
pub struct Handshake {
    pub owner: u8,
    pub ids: Vec<u8>,
}

impl Handshake {
    pub fn new(owner: u8, ids: Vec<u8>) -> Self {
        Handshake { owner, ids }
    }

    pub fn as_bytes(mut self) -> Vec<u8> {
        let mut buf_msg = Vec::<u8>::new();
        buf_msg.push(Messages::Handshake as u8);
        buf_msg.push(self.owner);
        buf_msg.push(self.ids.len() as u8);
        buf_msg.append(&mut self.ids);
        buf_msg.push(b'\n');
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Handshake {
        let owner = buffer[0];
        let len = buffer[1] as usize;
        let ids = match len {
            0 => vec![],
            1 => vec![buffer[2]],
            _ => buffer[2..2 + len].to_vec(),
        };

        Handshake { owner, ids }
    }
}

pub struct Coordinator {
    owner: u8,
}

impl Coordinator {
    pub fn new(owner: u8) -> Self {
        Coordinator { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::Coordinator as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Coordinator {
        let owner = buffer[0];

        Coordinator { owner }
    }
}

pub struct BullyOk {
    pub owner: u8,
}

impl BullyOk {
    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::BullyOk as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> BullyOk {
        let owner = buffer[0];

        BullyOk { owner }
    }
}

pub struct BullyElection {
    pub owner: u8,
}

impl BullyElection {
    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::BullyElection as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> BullyElection {
        let owner = buffer[0];

        BullyElection { owner }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub owner: u8,
    pub bucket: Bucket,
}

impl Token {
    pub fn new(owner: u8, bucket: Bucket) -> Self {
        Token { owner, bucket }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let mut buf_msg = Vec::<u8>::new();
        buf_msg.push(Messages::Token as u8);
        buf_msg.push(self.owner);
        buf_msg.extend_from_slice(&self.bucket.as_bytes());
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Result<Token, ParseError> {
        let owner = buffer[0];
        let bucket = Bucket::from_bytes(&buffer[1..6])?;

        Ok(Token { owner, bucket })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderJson {
    pub items: HashMap<IceCream, f32>,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub screen_id: u8,
    pub order_id: u8,
    pub items: HashMap<IceCream, f32>,
}

impl Order {
    pub fn new(screen_id: u8, order_id: u8, items: HashMap<IceCream, f32>) -> Self {
        Order {
            screen_id,
            order_id,
            items,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut buf_msg = vec![
            Messages::Order as u8,
            self.screen_id,
            self.order_id,
            self.items.len() as u8,
        ];
        for (ice_cream, amount) in self.items.iter() {
            buf_msg.push(*ice_cream as u8);
            buf_msg.extend_from_slice(&amount.to_be_bytes());
        }
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Order {
        let screen_id = buffer[0];
        let order_id = buffer[1];
        let len_items = buffer[2] as usize;
        let mut items = HashMap::new();

        for i in 0..len_items {
            let offset = 3 + i * 5;
            let icecream = IceCream::try_from(buffer[offset])
                .expect("Debe ser un número válido para el gusto de helado");
            let amount = f32::from_be_bytes(
                buffer[offset + 1..offset + 5]
                    .try_into()
                    .expect("Debía obtener los bytes que representan el f32"),
            );
            items.insert(icecream, amount);
        }

        Order {
            screen_id,
            order_id,
            items,
        }
    }
}

// =================================== OrderResult =========================================================== //
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderStatus {
    Ready = 0,
    Abort = 1,
}

impl TryFrom<u8> for OrderStatus {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OrderStatus::Ready),
            1 => Ok(OrderStatus::Abort),
            _ => Err(ParseError::ConversionError),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OrderResult {
    pub status: OrderStatus,
    pub order_id: u8,
    pub owner_id: u8,
}

impl OrderResult {
    pub fn new(status: OrderStatus, order_id: u8, owner_id: u8) -> Self {
        OrderResult {
            status,
            order_id,
            owner_id,
        }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![
            Messages::OrderResult as u8,
            self.status as u8,
            self.order_id,
            self.owner_id,
        ];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Result<OrderResult, ParseError> {
        let status = OrderStatus::try_from(buffer[0])?;
        let order_id = buffer[1];
        let owner_id = buffer[2];
        Ok(OrderResult {
            status,
            order_id,
            owner_id,
        })
    }
}

#[derive(Debug)]
pub struct RobotOrder {
    pub resolver_owner: u8,
    pub order: Order,
}

impl RobotOrder {
    pub fn new(
        resolver_owner: u8,
        screen_owner: u8,
        order_id: u8,
        items: HashMap<IceCream, f32>,
    ) -> Self {
        let order = Order::new(screen_owner, order_id, items);
        RobotOrder {
            resolver_owner,
            order,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut buf_msg = Vec::<u8>::new();
        buf_msg.push(Messages::RobotOrder as u8);
        buf_msg.push(self.resolver_owner);
        buf_msg.extend(self.order.as_bytes());
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> RobotOrder {
        let resolver_owner = buffer[0];
        let order = Order::from_bytes(&buffer[2..]);
        RobotOrder {
            resolver_owner,
            order,
        }
    }
}

pub struct KeepAlive {
    pub owner: u8,
}

impl KeepAlive {
    pub fn new(owner: u8) -> Self {
        KeepAlive { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::KeepAlive as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> KeepAlive {
        let owner = buffer[0];

        KeepAlive { owner }
    }
}

pub struct Alive {
    pub owner: u8,
}

impl Alive {
    pub fn new(owner: u8) -> Self {
        Alive { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::Alive as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> Alive {
        let owner = buffer[0];

        Alive { owner }
    }
}

pub struct RobotDead {
    pub owner: u8,
    pub status: Vec<(IceCream, Grade)>,
}

impl RobotDead {
    pub fn new(owner: u8, status: Vec<(IceCream, Grade)>) -> Self {
        RobotDead { owner, status }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let mut buf_msg = Vec::<u8>::new();
        buf_msg.push(Messages::RobotDead as u8);
        buf_msg.push(self.owner);
        buf_msg.push(self.status.len() as u8);
        for (ice_cream, status) in self.status.into_iter() {
            buf_msg.push(ice_cream as u8);
            buf_msg.push(status.clone() as u8);
        }
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> RobotDead {
        let owner = buffer[0];
        let len_status = buffer[1] as usize;
        let mut status = Vec::new();
        for i in 0..len_status {
            let icecream = IceCream::try_from(buffer[2 + 2 * i])
                .expect("Tiene que ser un numero valido para gusto de helado");
            let grade = Grade::try_from(buffer[3 + 2 * i])
                .expect("Tenia que ser un numero valido para status");
            status.push((icecream, grade));
        }

        RobotDead { owner, status }
    }
}

pub struct KeepAliveFromResolver {
    pub owner: u8,
}

impl KeepAliveFromResolver {
    pub fn new(owner: u8) -> Self {
        KeepAliveFromResolver { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::KeepAliveFromResolver as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> KeepAliveFromResolver {
        let owner = buffer[0];

        KeepAliveFromResolver { owner }
    }
}

pub struct OrderRequest {
    pub owner: u8,
}

impl OrderRequest {
    pub fn new(owner: u8) -> Self {
        OrderRequest { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::OrderRequest as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> OrderRequest {
        let owner = buffer[0];
        OrderRequest { owner }
    }
}

#[derive(Debug)]
pub struct RobotWithOrder {
    pub screen_id: u8,
    pub order_id: u8,
}

impl RobotWithOrder {
    pub fn new(screen_id: u8, order_id: u8) -> Self {
        RobotWithOrder {
            screen_id,
            order_id,
        }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![
            Messages::RobotWithOrder as u8,
            self.screen_id,
            self.order_id,
        ];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> RobotWithOrder {
        let screen_id = buffer[0];
        let order_id = buffer[1];
        RobotWithOrder {
            screen_id,
            order_id,
        }
    }
}

#[derive(Debug)]
pub struct NewLeader {
    pub owner: u8,
}

impl NewLeader {
    pub fn new(owner: u8) -> Self {
        NewLeader { owner }
    }

    pub fn as_bytes(self) -> Vec<u8> {
        let buf_msg = vec![Messages::NewLeader as u8, self.owner];
        buf_msg
    }

    pub fn from_bytes(buffer: &[u8]) -> NewLeader {
        let owner = buffer[0];
        NewLeader { owner }
    }
}

// ====================================== Gateway ============================================== //

#[derive(Debug, Clone)]
pub enum GatewayResponse {
    Acknowledge = 0,
    RejectedCard = 1,
    NoSuchPendingPayment = 2,
    DuplicatedOrder = 3,
}

impl TryFrom<u8> for GatewayResponse {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GatewayResponse::Acknowledge),
            1 => Ok(GatewayResponse::RejectedCard),
            2 => Ok(GatewayResponse::NoSuchPendingPayment),
            3 => Ok(GatewayResponse::DuplicatedOrder),
            _ => Err(ParseError::ConversionError),
        }
    }
}

impl From<GatewayResponse> for Vec<u8> {
    fn from(value: GatewayResponse) -> Self {
        match value {
            GatewayResponse::Acknowledge => {
                let bytes = vec![
                    Messages::GatewayResponse as u8,
                    GatewayResponse::Acknowledge as u8,
                ];
                bytes
            }
            GatewayResponse::RejectedCard => {
                let bytes = vec![
                    Messages::GatewayResponse as u8,
                    GatewayResponse::RejectedCard as u8,
                ];
                bytes
            }
            GatewayResponse::NoSuchPendingPayment => {
                let bytes = vec![
                    Messages::GatewayResponse as u8,
                    GatewayResponse::NoSuchPendingPayment as u8,
                ];
                bytes
            }
            GatewayResponse::DuplicatedOrder => {
                let bytes = vec![
                    Messages::GatewayResponse as u8,
                    GatewayResponse::DuplicatedOrder as u8,
                ];
                bytes
            }
        }
    }
}
