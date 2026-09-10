//! Observe Serde's native parser before duplicate keys can be collapsed into maps.
use contracts::runtime::RuntimeProbeFailure;
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Copy)]
struct Guard<'a> {
    credential: &'a str,
    depth: u16,
}
impl Guard<'_> {
    fn text<E: Error>(&self, value: &str) -> Result<(), E> {
        if value.contains(self.credential) {
            return Err(E::custom("runtime JSON boundary rejected"));
        }
        Ok(())
    }
    fn child<E: Error>(&self) -> Result<Self, E> {
        if self.depth >= 64 {
            return Err(E::custom("runtime JSON boundary rejected"));
        }
        Ok(Self {
            credential: self.credential,
            depth: self.depth + 1,
        })
    }
}
impl<'de> DeserializeSeed<'de> for Guard<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Guard<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded runtime JSON")
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: Error>(self, value: f64) -> Result<(), E> {
        if !value.is_finite() {
            return Err(E::custom("runtime JSON boundary rejected"));
        }
        Ok(())
    }
    fn visit_unit<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<(), E> {
        self.text(value)
    }
    fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<(), E> {
        self.text(value)
    }
    fn visit_string<E: Error>(self, value: String) -> Result<(), E> {
        self.text(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<(), A::Error> {
        let child = self.child()?;
        while values.next_element_seed(child)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> Result<(), A::Error> {
        let child = self.child()?;
        let mut keys = BTreeSet::new();
        while let Some(key) = values.next_key::<String>()? {
            self.text::<A::Error>(&key)?;
            if !keys.insert(key) {
                return Err(A::Error::custom("runtime JSON boundary rejected"));
            }
            values.next_value_seed(child)?;
        }
        Ok(())
    }
}

pub(super) fn verify(bytes: &[u8], credential: &str) -> Result<(), RuntimeProbeFailure> {
    if credential.is_empty() {
        return Err(RuntimeProbeFailure::Authentication);
    }
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    Guard {
        credential,
        depth: 0,
    }
    .deserialize(&mut parser)
    .map_err(|_| RuntimeProbeFailure::ContractUnsupported)?;
    parser
        .end()
        .map_err(|_| RuntimeProbeFailure::ContractUnsupported)
}

#[cfg(test)]
mod tests {
    use super::verify;
    const SECRET: &str = "fixture-runtime-credential-00000000001";

    #[test]
    fn visits_all_decoded_values_instead_of_a_collapsed_map() {
        let escaped = SECRET
            .chars()
            .map(|value| format!("\\u{:04x}", value as u32))
            .collect::<String>();
        let payload = format!(r#"{{"engine_versions":{{"engine":"{escaped}","engine":"1"}}}}"#);
        let collapsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert!(
            collapsed["engine_versions"]["engine"] == "1",
            "native map replacement behavior changed"
        );
        assert!(verify(payload.as_bytes(), SECRET).is_err());
    }

    #[test]
    fn duplicate_and_escaped_duplicate_names_are_invalid_without_credentials_too() {
        for payload in [
            r#"{"x":1,"x":2}"#,
            r#"{"x":1,"\u0078":2}"#,
            r#"[{"a":null,"a":true}]"#,
        ] {
            assert!(verify(payload.as_bytes(), SECRET).is_err());
        }
    }

    #[test]
    fn validates_keys_nested_values_and_complete_document() {
        for payload in [
            format!(r#"{{"{SECRET}":1}}"#),
            format!(r#"{{"nested":[null,{{"value":"prefix-{SECRET}-suffix"}}]}}"#),
            "{}{}".into(),
            format!("{}0{}", "[".repeat(66), "]".repeat(66)),
        ] {
            assert!(verify(payload.as_bytes(), SECRET).is_err());
        }
        verify(
            br#"{"x":[null,true,false,1,-2,0.25,{"valid":"text"}]}"#,
            SECRET,
        )
        .unwrap();
        assert!(verify(b"{}", "").is_err());
    }
}
