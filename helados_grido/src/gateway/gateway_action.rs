use std::convert::{TryFrom, TryInto};

use crate::utils::errors::ParseError;

#[derive(Debug, Clone)]
pub enum GatewayAction {
    Capture {
        order_id: u8,
        card_number: u32,
        amount: f64,
        owner_id: u8,
    },
    Commit {
        order_id: u8,
        owner_id: u8,
    },
    Abort {
        order_id: u8,
        owner_id: u8,
    },
}

impl TryFrom<Vec<u8>> for GatewayAction {
    type Error = ParseError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        GatewayAction::try_from(&bytes[..])
    }
}

impl TryFrom<&[u8]> for GatewayAction {
    type Error = ParseError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.is_empty() {
            return Err(ParseError::EmptyPacket);
        }

        match bytes[0] {
            0 => {
                if bytes.len() < 15 {
                    return Err(ParseError::IncompleteCapturePacket);
                }

                let card_number = u32::from_be_bytes(
                    bytes[2..6]
                        .try_into()
                        .map_err(|_| ParseError::ConversionError)?,
                );
                let amount = f64::from_be_bytes(
                    bytes[6..14]
                        .try_into()
                        .map_err(|_| ParseError::ConversionError)?,
                );

                Ok(GatewayAction::Capture {
                    order_id: bytes[1],
                    card_number,
                    amount,
                    owner_id: bytes[14],
                })
            }
            1 => {
                if bytes.len() < 3 {
                    return Err(ParseError::IncompleteCommitPacket);
                }

                Ok(GatewayAction::Commit {
                    order_id: bytes[1],
                    owner_id: bytes[2],
                })
            }
            2 => {
                if bytes.len() < 3 {
                    return Err(ParseError::IncompleteAbortPacket);
                }

                Ok(GatewayAction::Abort {
                    order_id: bytes[1],
                    owner_id: bytes[2],
                })
            }
            _ => Err(ParseError::UnknownPacket),
        }
    }
}

impl From<GatewayAction> for Vec<u8> {
    fn from(value: GatewayAction) -> Self {
        match value {
            GatewayAction::Capture {
                order_id,
                card_number,
                amount,
                owner_id,
            } => {
                let mut bytes = Vec::new();
                bytes.push(0);
                bytes.push(order_id);
                bytes.extend_from_slice(&card_number.to_be_bytes());
                bytes.extend_from_slice(&amount.to_be_bytes());
                bytes.push(owner_id);
                bytes
            }
            GatewayAction::Commit { order_id, owner_id } => {
                let bytes = vec![1, order_id, owner_id];
                bytes
            }
            GatewayAction::Abort { order_id, owner_id } => {
                let bytes = vec![2, order_id, owner_id];
                bytes
            }
        }
    }
}
