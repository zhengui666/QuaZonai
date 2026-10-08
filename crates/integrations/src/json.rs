//! Observe Serde's native parser before duplicate keys can be collapsed into maps.
use contracts::runtime::RuntimeProbeFailure;
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Copy)]
struct Guard<'a> {
    credential: &'a str,
}
impl Guard<'_> {
    fn text<E: Error>(&self, value: &str) -> Result<(), E> {
        if value.contains(self.credential) {
            return Err(E::custom("runtime JSON boundary rejected"));
        }
        Ok(())
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
        while values.next_element_seed(self)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = values.next_key::<String>()? {
            self.text::<A::Error>(&key)?;
            if !keys.insert(key) {
                return Err(A::Error::custom("runtime JSON boundary rejected"));
            }
            values.next_value_seed(self)?;
        }
        Ok(())
    }
}

pub fn verify(bytes: &[u8], credential: &str) -> Result<(), RuntimeProbeFailure> {
    if credential.is_empty() {
        return Err(RuntimeProbeFailure::Authentication);
    }
    // Keep Serde JSON's native 128-level recursion budget. It checks each
    // container before entering this visitor, including every seeded child.
    // No separate application nesting quota or unchecked recursion is added.
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    Guard { credential }
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

    fn nested(depth: usize, value: &str) -> String {
        format!("{}{value}{}", "[".repeat(depth), "]".repeat(depth))
    }

    #[test]
    fn native_parser_is_the_only_nesting_boundary() {
        for depth in [65, 100, 127] {
            let payload = nested(depth, "0");
            assert!(serde_json::from_str::<serde_json::Value>(&payload).is_ok());
            verify(payload.as_bytes(), SECRET).unwrap();
        }
        for depth in [128, 10_000] {
            let payload = nested(depth, "0");
            let error = serde_json::from_str::<serde_json::Value>(&payload).unwrap_err();
            assert!(error.to_string().contains("recursion limit exceeded"));
            assert!(verify(payload.as_bytes(), SECRET).is_err());
        }
    }

    #[test]
    fn complete_deep_documents_keep_duplicate_secret_and_finite_checks() {
        let escaped = SECRET
            .chars()
            .map(|value| format!("\\u{:04x}", value as u32))
            .collect::<String>();
        for value in [
            r#"{"x":1,"\u0078":2}"#.to_owned(),
            format!(r#"{{"{escaped}":1}}"#),
            format!(r#"{{"value":"prefix-{escaped}-suffix"}}"#),
            "1e400".to_owned(),
            r#"{"x":}"#.to_owned(),
        ] {
            assert!(verify(nested(100, &value).as_bytes(), SECRET).is_err());
        }
        let complete = nested(100, r#"{"value":"complete"}"#);
        verify(complete.as_bytes(), SECRET).unwrap();
        assert!(verify(format!("{complete}{{}}").as_bytes(), SECRET).is_err());
        assert!(verify(complete[..complete.len() - 1].as_bytes(), SECRET).is_err());
    }
}
