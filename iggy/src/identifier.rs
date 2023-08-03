use crate::bytes_serializable::BytesSerializable;
use crate::error::Error;
use serde::{Deserialize, Serialize};
use serde_with::base64::Base64;
use serde_with::serde_as;
use std::fmt::Display;
use std::str::FromStr;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Identifier {
    pub kind: IdKind,
    #[serde(skip)]
    pub length: u8,
    #[serde_as(as = "Base64")]
    pub value: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    #[default]
    Numeric,
    String,
}

impl Default for Identifier {
    fn default() -> Self {
        Self {
            kind: IdKind::default(),
            length: 4,
            value: 1u32.to_le_bytes().to_vec(),
        }
    }
}

impl Identifier {
    pub fn get_u32_value(&self) -> Result<u32, Error> {
        if self.kind != IdKind::Numeric {
            return Err(Error::InvalidCommand);
        }

        if self.length != 4 {
            return Err(Error::InvalidCommand);
        }

        Ok(u32::from_le_bytes(self.value.clone().try_into().unwrap()))
    }

    pub fn get_string_value(&self) -> Result<String, Error> {
        if self.kind != IdKind::String {
            return Err(Error::InvalidCommand);
        }

        Ok(String::from_utf8_lossy(&self.value).to_string())
    }

    pub fn as_string(&self) -> String {
        match self.kind {
            IdKind::Numeric => self.get_u32_value().unwrap().to_string(),
            IdKind::String => self.get_string_value().unwrap(),
        }
    }

    pub fn get_size_bytes(&self) -> u32 {
        2 + self.length as u32
    }

    pub fn from_identifier(identifier: &Identifier) -> Self {
        Self {
            kind: identifier.kind,
            length: identifier.length,
            value: identifier.value.clone(),
        }
    }

    pub fn from_str_value(value: &str) -> Result<Self, Error> {
        let length = value.len();
        if length == 0 || length > 255 {
            return Err(Error::InvalidCommand);
        }

        match value.parse::<u32>() {
            Ok(id) => Identifier::numeric(id),
            Err(_) => Identifier::string(&value),
        }
    }

    pub fn numeric(value: u32) -> Result<Self, Error> {
        if value == 0 {
            return Err(Error::InvalidCommand);
        }

        Ok(Self {
            kind: IdKind::Numeric,
            length: 4,
            value: value.to_le_bytes().to_vec(),
        })
    }

    pub fn string(value: &str) -> Result<Self, Error> {
        let length = value.len();
        if length == 0 || length > 255 {
            return Err(Error::InvalidCommand);
        }

        Ok(Self {
            kind: IdKind::String,
            length: length as u8,
            value: value.as_bytes().to_vec(),
        })
    }
}

impl BytesSerializable for Identifier {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.length as usize);
        bytes.extend(self.kind.as_code().to_le_bytes());
        bytes.extend(self.length.to_le_bytes());
        bytes.extend(&self.value);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Error>
    where
        Self: Sized,
    {
        if bytes.len() < 3 {
            return Err(Error::InvalidCommand);
        }

        let kind = IdKind::from_code(bytes[0])?;
        let length = bytes[1];
        let value = bytes[2..2 + length as usize].to_vec();
        if value.len() != length as usize {
            return Err(Error::InvalidCommand);
        }

        Ok(Identifier {
            kind,
            length,
            value,
        })
    }
}

impl IdKind {
    pub fn as_code(&self) -> u8 {
        match self {
            IdKind::Numeric => 1,
            IdKind::String => 2,
        }
    }

    pub fn from_code(code: u8) -> Result<Self, Error> {
        match code {
            1 => Ok(IdKind::Numeric),
            2 => Ok(IdKind::String),
            _ => Err(Error::InvalidCommand),
        }
    }
}

impl FromStr for IdKind {
    type Err = Error;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "n" | "numeric" => Ok(IdKind::Numeric),
            "s" | "string" => Ok(IdKind::String),
            _ => Err(Error::InvalidCommand),
        }
    }
}

impl FromStr for Identifier {
    type Err = Error;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if let Ok(value) = input.parse::<u32>() {
            return Identifier::numeric(value);
        }

        Identifier::string(input)
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            IdKind::Numeric => write!(
                f,
                "{}",
                u32::from_le_bytes(self.value.as_slice().try_into().unwrap())
            ),
            IdKind::String => write!(f, "{}", String::from_utf8_lossy(&self.value)),
        }
    }
}

impl Display for IdKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdKind::Numeric => write!(f, "numeric"),
            IdKind::String => write!(f, "string"),
        }
    }
}