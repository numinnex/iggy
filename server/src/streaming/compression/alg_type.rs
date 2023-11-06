use std::str::FromStr;
use serde::{Serialize, Serializer, de::{Visitor, self, Deserializer}, Deserialize};

// for now only those, in the future will add snappy, lz4, zstd (same as in kafka) in addition to that
// we should consider brotli aswell.
#[derive(Debug, PartialEq, Clone)]
pub enum CompressionAlg {
    Producer,
    Gzip,
}

impl Default for CompressionAlg {
    fn default() -> Self {
        CompressionAlg::Producer
    }
}

impl FromStr for CompressionAlg { 
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Producer" | "producer" => Ok(CompressionAlg::Producer),
            "Gzip" | "gzip" => Ok(CompressionAlg::Gzip),
            _ => Err(format!("Unknown compression type: {}", s))
        }
    }
}

impl Serialize for CompressionAlg {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            CompressionAlg::Producer => serializer.serialize_str("producer"),
            CompressionAlg::Gzip => serializer.serialize_str("gzip"),
        }
    }
}
struct CompressionAlgVisitor; 

impl<'de> Visitor<'de> for CompressionAlgVisitor {
    type Value = CompressionAlg;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid compression type, check documentation for more information.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        CompressionAlg::from_str(&value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CompressionAlg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CompressionAlgVisitor)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialize() {
        let producer_alg = CompressionAlg::Producer;
        let producer_serialized = serde_json::to_string(&producer_alg).unwrap();

        //gzip alg
        let gzip_alg = CompressionAlg::Gzip;
        let gzip_serialized = serde_json::to_string(&gzip_alg).unwrap();
        assert_eq!(producer_serialized, json!("producer").to_string());
        assert_eq!(gzip_serialized, json!("gzip").to_string());
    }

    #[test]
    fn test_deserialize() {
        let json_data = "\"producer\"";
        let deserialized: Result<CompressionAlg, serde_json::Error> =
            serde_json::from_str(json_data);
        assert!(deserialized.is_ok());

        let json_data = "\"Producer\"";
        let deserialized: Result<CompressionAlg, serde_json::Error> =
            serde_json::from_str(json_data);
        assert!(deserialized.is_ok());
        assert_eq!(deserialized.unwrap(), CompressionAlg::Producer);

        let json_data = "\"Gzip\"";
        let deserialized: Result<CompressionAlg, serde_json::Error> =
            serde_json::from_str(json_data);
        assert!(deserialized.is_ok());

        let json_data = "\"gzip\"";
        let deserialized: Result<CompressionAlg, serde_json::Error> =
            serde_json::from_str(json_data);
        assert!(deserialized.is_ok());
        assert_eq!(deserialized.unwrap(), CompressionAlg::Gzip);
    }
}