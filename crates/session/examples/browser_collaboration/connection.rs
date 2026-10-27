//! Example connection intents and validation over the shared session API.
use super::*;

#[derive(Clone)]
pub(super) struct Attempt {
    pub endpoint: String,
    pub request: ClientRequest<Cube>,
}

/// Validate a create/join form before replacing any active connection.
pub(super) fn prepare(
    endpoint: &str,
    name: &str,
    code: &str,
    join: bool,
    secure: bool,
) -> Result<Attempt, String> {
    let endpoint = dimensify_transport::client::Endpoint::parse(endpoint.trim())
        .map_err(|_| "Enter a ws:// or wss:// host URL without credentials or a fragment.")?
        .websocket;
    if secure && !endpoint.starts_with("wss://") {
        return Err("This HTTPS page needs a wss:// host URL.".into());
    }
    if !dimensify_session::valid_display_name(name) {
        return Err("Enter a display name of 1-64 bytes without control characters.".into());
    }
    let name = name.trim().to_string();
    let request = if join {
        let code = InviteCode::parse(code.trim())
            .map_err(|_| "Enter a valid 12-character invite code to join a room.")?;
        ClientRequest::JoinWithCode { code, name }
    } else {
        ClientRequest::Create { name }
    };
    Ok(Attempt { endpoint, request })
}

#[derive(Resource, Default)]
pub(super) struct Panel {
    pub open: bool,
    pub pending: Option<(Attempt, bool)>,
    pub last: Option<Attempt>,
}

/// Queue the URL/command-line attempt using the same validation as the form.
pub(super) fn startup(mut panel: ResMut<Panel>, options: Res<Options>, mut demo: ResMut<Demo>) {
    match prepare(
        &options.endpoint,
        &options.name,
        &options.code,
        !options.code.is_empty(),
        secure_page(),
    ) {
        Ok(attempt) => panel.pending = Some((attempt, false)),
        Err(error) => {
            demo.error = error;
            panel.open = true;
        }
    }
}

/// Apply a validated user attempt, keeping retry credentials bound to the shared API.
pub(super) fn apply(world: &mut World) {
    let Some((attempt, retry)) = world.resource_mut::<Panel>().pending.take() else {
        return;
    };
    if !retry {
        dimensify_session::disconnect::<Cube>(world);
    }
    let result = dimensify_session::connect::<Cube>(world, &attempt.endpoint).and_then(|()| {
        let mut session = world.resource_mut::<ClientSession<Cube>>();
        if session.resume_credential().is_none() {
            session.send(attempt.request.clone())?;
        }
        Ok(())
    });
    match result {
        Ok(()) => {
            world.resource_mut::<Options>().endpoint = attempt.endpoint.clone();
            let mut panel = world.resource_mut::<Panel>();
            panel.last = Some(attempt);
            panel.open = false;
            *world.resource_mut::<Demo>() = Demo::default();
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .clear();
        }
        Err(error) => world.resource_mut::<Demo>().error = error.to_string(),
    }
}

/// Detect the browser's mixed-content boundary without imposing it on native clients.
pub(super) fn secure_page() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().is_some_and(|window| {
            window
                .location()
                .protocol()
                .is_ok_and(|scheme| scheme == "https:")
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_trims_fields_and_ignores_old_invite() {
        let attempt = prepare(" ws://localhost:5002 ", " host ", "old-code", false, false).unwrap();
        assert_eq!(attempt.endpoint, "ws://localhost:5002/");
        assert!(matches!(attempt.request, ClientRequest::Create { name } if name == "host"));
    }
    #[test]
    fn join_requires_an_invite_instead_of_creating_a_room() {
        assert!(prepare("ws://localhost:5002", "guest", "", true, false).is_err());
        let attempt = prepare(
            "ws://localhost:5002",
            "guest",
            "ABCD-EFGH-JKMN",
            true,
            false,
        )
        .unwrap();
        assert!(matches!(
            attempt.request,
            ClientRequest::JoinWithCode { .. }
        ));
    }
    #[test]
    fn rejects_invalid_endpoints_names_and_mixed_content() {
        for endpoint in [
            "",
            "https://localhost",
            "ws://user:pass@localhost",
            "ws://localhost/#fragment",
        ] {
            assert!(prepare(endpoint, "guest", "", false, false).is_err());
        }
        for name in ["".to_string(), "a".repeat(65), "bad\nname".into()] {
            assert!(prepare("ws://localhost:5002", &name, "", false, false).is_err());
        }
        assert!(prepare("ws://localhost:5002", "guest", "", false, true).is_err());
        assert!(prepare("wss://localhost:5002", "guest", "", false, true).is_ok());
    }
}
