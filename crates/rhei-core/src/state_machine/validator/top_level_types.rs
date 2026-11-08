// The decoders that hold `name` and `version` to the types their rows name, so a
// value of another type, a null included, is refused at load naming the field.
// §FS-rhei-states.1.1

/// Decode `name` as a string and nothing else. A plain `String` would take a null
/// as `""` and a number or bool as its text. §FS-rhei-states.1.1
fn deserialize_machine_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct NameVisitor;

    impl serde::de::Visitor<'_> for NameVisitor {
        type Value = String;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            Ok(value.to_string())
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<String, E> {
            Err(E::invalid_type(serde::de::Unexpected::Other("null"), &self))
        }
    }

    deserializer.deserialize_any(NameVisitor)
}

/// Decode `version` as a number or a string, kept as written, and refuse a null,
/// a bool, a sequence or a mapping. §FS-rhei-states.1.1
fn deserialize_machine_version<'de, D>(deserializer: D) -> Result<serde_yaml::Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct VersionVisitor;

    impl serde::de::Visitor<'_> for VersionVisitor {
        type Value = serde_yaml::Value;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a number or a string")
        }

        fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(serde_yaml::Value::Number(value.into()))
        }

        fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
            Ok(serde_yaml::Value::Number(value.into()))
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
            Ok(serde_yaml::Value::Number(value.into()))
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(serde_yaml::Value::String(value.to_string()))
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Err(E::invalid_type(serde::de::Unexpected::Other("null"), &self))
        }
    }

    deserializer.deserialize_any(VersionVisitor)
}
