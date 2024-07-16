use crate::utils::errors::ParseError;
use serde::{Deserialize, Serialize};

#[derive(Eq, Hash, PartialEq, Clone, Debug, Copy, Serialize, Deserialize)]
pub enum IceCream {
    Chocolate = 0,
    Vanilla = 1,
    Strawberry = 2,
    Lemon = 3,
    DulceDeLeche = 4,
}

impl TryFrom<u8> for IceCream {
    type Error = crate::utils::errors::ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(IceCream::Chocolate),
            1 => Ok(IceCream::Vanilla),
            2 => Ok(IceCream::Strawberry),
            3 => Ok(IceCream::Lemon),
            4 => Ok(IceCream::DulceDeLeche),
            _ => Err(crate::utils::errors::ParseError::ConversionError),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bucket {
    pub ice_cream: IceCream,
    pub amount: f32,
}

impl Bucket {
    pub fn new(ice_cream: IceCream, amount: f32) -> Self {
        Bucket { ice_cream, amount }
    }

    pub fn as_bytes(self) -> [u8; 5] {
        let mut array: [u8; 5] = [self.ice_cream as u8, 0, 0, 0, 0];
        let amount_in_be: [u8; 4] = self.amount.to_be_bytes();

        for (i, byte) in amount_in_be.iter().enumerate() {
            array[i + 1] = *byte;
        }

        array
    }

    pub fn from_bytes(buffer: &[u8]) -> Result<Bucket, ParseError> {
        let ice_cream = IceCream::try_from(buffer[0])?;
        let amount = f32::from_be_bytes(buffer[1..5].try_into()?);
        Ok(Bucket::new(ice_cream, amount))
    }
}
