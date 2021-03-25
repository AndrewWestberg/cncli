use rug::float::Round;
use rug::ops::MulAssignRound;
use rug::{Float, Rational};
use serde::{Deserialize, Deserializer};
use serde_cbor::{de, Value};
use serde_json::Number;

pub(crate) fn rational<'de, D: Deserializer<'de>>(d: D) -> Result<Rational, D::Error> {
    let n: Number = Deserialize::deserialize(d)?;
    let mut f: Float = Float::with_val(24, Float::parse(&*n.to_string()).unwrap());
    f.mul_assign_round(100, Round::Nearest);
    Ok(Rational::from((f.to_integer().unwrap(), 100)))
}

// pub(crate) fn rational_optional<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Rational>, D::Error> {
//     let n: Option<Number> = Deserialize::deserialize(d)?;
//     match n {
//         None => Ok(None),
//         Some(number) => {
//             let mut f: Float = Float::with_val(24, Float::parse(&*number.to_string()).unwrap());
//             f.mul_assign_round(100, Round::Nearest);
//             Ok(Some(Rational::from((f.to_integer().unwrap(), 100))))
//         }
//     }
// }

pub(crate) fn cbor_hex<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let cbor: String = Deserialize::deserialize(d)?;
    let cbor_vec = hex::decode(cbor).unwrap();
    let value: Value = de::from_slice(&*cbor_vec).unwrap();
    match value {
        Value::Bytes(key) => Ok(key),
        _ => {
            panic!("Invalid cbor hex!")
        }
    }
}
