use crate::*;
use bevy::prelude::*;
use lightyear::prelude::*;
pub(crate) struct ControlChannel;
pub(crate) struct CameraChannel;
pub(crate) struct CursorChannel;
#[derive(Default)]
pub(crate) struct ProtocolPlugin<D: SharedDocument>(std::marker::PhantomData<D>);
impl<D: SharedDocument> Plugin for ProtocolPlugin<D> {
    fn build(&self, app: &mut App) {
        app.register_message::<ClientEnvelope<D>>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerEnvelope<D>>()
            .add_direction(NetworkDirection::ServerToClient);
        app.add_channel::<ControlChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
        app.add_channel::<CursorChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
        app.add_channel::<CameraChannel>(ChannelSettings {
            mode: ChannelMode::SequencedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
    }
}

// Validate the schema tag before deserializing the application-specific payload.
// The protocol prefix versions the room/control envelope independently of D.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "T: serde::Serialize",
    deserialize = "T: serde::Deserialize<'de>"
))]
pub(crate) struct Envelope<D: SharedDocument, T> {
    schema: Schema<D>,
    pub message: T,
}
impl<D: SharedDocument, T> Envelope<D, T> {
    /// Wrap a message with the document schema and registration contract.
    pub fn new(message: T) -> Self {
        Self {
            schema: Schema(std::marker::PhantomData),
            message,
        }
    }
}
#[derive(Clone)]
struct Schema<D>(std::marker::PhantomData<D>);
impl<D: SharedDocument> serde::Serialize for Schema<D> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let manifest = D::registrations()
            .manifest()
            .map_err(serde::ser::Error::custom)?;
        let tag = format!("collaboration.v9/{}/{}", D::SCHEMA, manifest);
        if tag.len() > MAX_TEXT {
            return Err(serde::ser::Error::custom("schema manifest too long"));
        }
        serializer.serialize_str(&tag)
    }
}
impl<'de, D: SharedDocument> serde::Deserialize<'de> for Schema<D> {
    fn deserialize<S: serde::Deserializer<'de>>(deserializer: S) -> Result<Self, S::Error> {
        let tag = bounded_text(deserializer)?;
        let manifest = D::registrations()
            .manifest()
            .map_err(serde::de::Error::custom)?;
        if tag != format!("collaboration.v9/{}/{}", D::SCHEMA, manifest) {
            return Err(serde::de::Error::custom(
                "incompatible collaboration schema",
            ));
        }
        Ok(Self(std::marker::PhantomData))
    }
}
pub(crate) type ClientEnvelope<D> = Envelope<D, ClientRequest<D>>;
pub(crate) type ServerEnvelope<D> = Envelope<D, ServerMessage<D>>;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    pub(crate) struct TestDocument;
    impl SharedDocument for TestDocument {
        const SCHEMA: &'static str = "test.v1";
        fn registrations() -> RegistrationSet<Self> {
            RegistrationSet::default().presentation_camera()
        }
        fn validate(&self) -> bool {
            true
        }
    }
    #[test]
    fn old_control_envelope_is_rejected_even_with_matching_document_schema() {
        let tag = format!(
            "collaboration.v8/{}/{}",
            TestDocument::SCHEMA,
            TestDocument::registrations().manifest().unwrap()
        );
        let bytes = postcard::to_allocvec(&(
            tag,
            ClientRequest::<TestDocument>::Create {
                name: "host".into(),
            },
        ))
        .unwrap();
        assert!(postcard::from_bytes::<ClientEnvelope<TestDocument>>(&bytes).is_err());
    }
    #[test]
    fn incompatible_schema_rejected_before_payload_decoding() {
        let msg = Envelope::<TestDocument, _>::new(ClientRequest::<TestDocument>::Create {
            name: "host".into(),
        });
        let bytes = postcard::to_allocvec(&msg).unwrap();
        assert!(postcard::from_bytes::<ClientEnvelope<TestDocument>>(&bytes).is_ok());
        // Same-layout application with a distinct schema must be rejected too.
        #[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
        struct Other;
        impl SharedDocument for Other {
            const SCHEMA: &'static str = "other.v1";
            fn validate(&self) -> bool {
                true
            }
        }
        assert!(postcard::from_bytes::<ClientEnvelope<Other>>(&bytes).is_err());
        #[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
        struct SameSchemaWithoutCamera;
        impl SharedDocument for SameSchemaWithoutCamera {
            const SCHEMA: &'static str = "test.v1";
            fn validate(&self) -> bool {
                true
            }
        }
        assert!(postcard::from_bytes::<ClientEnvelope<SameSchemaWithoutCamera>>(&bytes).is_err());
        // A truncated payload after a mismatched tag must fail on the tag first.
        let wrong_tag = postcard::to_allocvec("collaboration.v2/other.v1").unwrap();
        let error = postcard::from_bytes::<ClientEnvelope<TestDocument>>(&wrong_tag)
            .err()
            .unwrap();
        assert_eq!(error, postcard::Error::SerdeDeCustom);
    }
}
