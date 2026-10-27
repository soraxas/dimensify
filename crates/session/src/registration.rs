//! Static protocol opt-in and local adapter installation. No wire payload erasure.
use crate::{SessionError, SharedDocument};
use bevy::prelude::*;
use std::{any::TypeId, collections::HashMap};

pub const CAMERA_ID: &str = "presentation.camera";
pub const CAMERA_VERSION: u32 = 1;
pub const MAX_REGISTRATIONS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePolicy {
    Editors,
    Presenter,
}
impl WritePolicy {
    /// Check whether the supplied editor and presenter rights permit writing.
    pub fn permits(self, editor: bool, presenter: bool) -> bool {
        match self {
            Self::Editors => editor,
            Self::Presenter => presenter,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedChannel {
    Document,
    PresentationCamera,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationDescriptor {
    pub id: &'static str,
    pub version: u32,
    pub policy: WritePolicy,
    pub channel: SharedChannel,
}
/// A typed projection selects exactly the application state this protocol shares.
pub trait DocumentProjection<D: SharedDocument>: 'static {
    type Payload: Clone + PartialEq;
    const ID: &'static str;
    const VERSION: u32;
    fn read(document: &D) -> Self::Payload;
    fn replace(document: &mut D, payload: Self::Payload);
    fn validate(payload: &Self::Payload) -> bool;
}
struct Registration<D> {
    descriptor: RegistrationDescriptor,
    changed: fn(&D, &D) -> bool,
    replace: fn(&mut D, &D) -> Result<(), SessionError>,
}
pub struct RegistrationSet<D> {
    entries: Vec<Registration<D>>,
}
impl<D> Default for RegistrationSet<D> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
impl<D: SharedDocument> RegistrationSet<D> {
    /// Register a typed document projection with an explicit write policy.
    pub fn document<P: DocumentProjection<D>>(mut self, policy: WritePolicy) -> Self {
        self.entries.push(Registration {
            descriptor: RegistrationDescriptor {
                id: P::ID,
                version: P::VERSION,
                policy,
                channel: SharedChannel::Document,
            },
            changed: |old, next| P::read(old) != P::read(next),
            replace: |old, next| {
                let payload = P::read(next);
                if !P::validate(&payload) {
                    return Err(SessionError::Invalid);
                }
                P::replace(old, payload);
                Ok(())
            },
        });
        self
    }
    /// Opt into the shared presentation camera channel.
    pub fn presentation_camera(mut self) -> Self {
        self.entries.push(Registration {
            descriptor: RegistrationDescriptor {
                id: CAMERA_ID,
                version: CAMERA_VERSION,
                policy: WritePolicy::Presenter,
                channel: SharedChannel::PresentationCamera,
            },
            changed: |_, _| false,
            replace: |_, _| Ok(()),
        });
        self
    }
    /// Validate registrations and return their canonical sorted descriptors.
    pub fn descriptors(&self) -> Result<Vec<RegistrationDescriptor>, SessionError> {
        if self.entries.len() > MAX_REGISTRATIONS {
            return Err(SessionError::Invalid);
        }
        let mut descriptors: Vec<_> = self.entries.iter().map(|r| r.descriptor).collect();
        descriptors.sort_by_key(|r| r.id);
        if descriptors.iter().enumerate().any(|(i, r)| {
            r.id.is_empty()
                || r.id.len() > 64
                || r.version == 0
                || !r
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || (i > 0 && descriptors[i - 1].id == r.id)
                || (r.id == CAMERA_ID && r.channel != SharedChannel::PresentationCamera)
        }) {
            return Err(SessionError::Invalid);
        }
        Ok(descriptors)
    }
    /// Canonical and bounded compatibility contract (order independent).
    pub fn manifest(&self) -> Result<String, SessionError> {
        Ok(self
            .descriptors()?
            .iter()
            .map(|r| format!("{}:{}:{:?}:{:?}", r.id, r.version, r.policy, r.channel))
            .collect::<Vec<_>>()
            .join(","))
    }
    /// Report whether a valid presentation camera registration is enabled.
    pub fn has_camera(&self) -> bool {
        self.descriptors().is_ok_and(|ds| {
            ds.iter()
                .any(|r| r.channel == SharedChannel::PresentationCamera)
        })
    }
    /// Resolve the write policy of a registered document projection.
    pub fn policy<P: DocumentProjection<D>>(&self) -> Result<WritePolicy, SessionError> {
        self.descriptors()?
            .into_iter()
            .find(|r| {
                r.id == P::ID && r.version == P::VERSION && r.channel == SharedChannel::Document
            })
            .map(|r| r.policy)
            .ok_or(SessionError::Unauthorized)
    }
    /// Reject changes to unregistered projections or projections without write permission.
    pub(crate) fn authorize(
        &self,
        old: &D,
        proposed: &D,
        editor: bool,
        presenter: bool,
    ) -> Result<(), SessionError> {
        self.descriptors()?;
        let mut permitted = false;
        for r in &self.entries {
            if r.descriptor.channel != SharedChannel::Document {
                continue;
            }
            let allowed = r.descriptor.policy.permits(editor, presenter);
            permitted |= allowed;
            if (r.changed)(old, proposed) && !allowed {
                return Err(SessionError::Unauthorized);
            }
        }
        if !permitted {
            return Err(SessionError::Unauthorized);
        }
        Ok(())
    }
    /// Rebuild from the old document, so changes to unregistered fields fail closed.
    pub fn apply(
        &self,
        old: &D,
        proposed: &D,
        editor: bool,
        presenter: bool,
    ) -> Result<D, SessionError> {
        self.authorize(old, proposed, editor, presenter)?;
        let mut rebuilt = old.clone();
        for r in &self.entries {
            if r.descriptor.channel != SharedChannel::Document {
                continue;
            }
            if (r.changed)(old, proposed) {
                (r.replace)(&mut rebuilt, proposed)?;
            }
        }
        if rebuilt != *proposed {
            return Err(SessionError::Unauthorized);
        }
        if !rebuilt.validate() {
            return Err(SessionError::Invalid);
        }
        Ok(rebuilt)
    }
}
/// Installing an adapter never selects every entity of its domain type.
pub trait SharedAdapter: 'static {
    type Document: SharedDocument;
    const ID: &'static str;
    const VERSION: u32;
    const POLICY: WritePolicy;
    const CHANNEL: SharedChannel;
    fn install(app: &mut App);
}
#[derive(Resource, Default)]
struct InstalledAdapters(HashMap<(TypeId, &'static str), TypeId>);
pub trait SharedAppExt {
    fn register_shared<A: SharedAdapter>(&mut self) -> Result<&mut Self, SessionError>;
}
impl SharedAppExt for App {
    fn register_shared<A: SharedAdapter>(&mut self) -> Result<&mut Self, SessionError> {
        let descriptor = RegistrationDescriptor {
            id: A::ID,
            version: A::VERSION,
            policy: A::POLICY,
            channel: A::CHANNEL,
        };
        if !A::Document::registrations()
            .descriptors()?
            .contains(&descriptor)
        {
            return Err(SessionError::Invalid);
        }
        self.init_resource::<InstalledAdapters>();
        let key = (TypeId::of::<A::Document>(), A::ID);
        let previous = self
            .world()
            .resource::<InstalledAdapters>()
            .0
            .get(&key)
            .copied();
        if let Some(previous) = previous {
            if previous != TypeId::of::<A>() {
                return Err(SessionError::Invalid);
            }
        } else {
            self.world_mut()
                .resource_mut::<InstalledAdapters>()
                .0
                .insert(key, TypeId::of::<A>());
            A::install(self);
        }
        Ok(self)
    }
}
