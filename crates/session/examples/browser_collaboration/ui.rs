use super::*;
use bevy::{
    feathers::{controls::FeathersButton, theme::ThemedText},
    picking::hover::Hovered,
    ui_widgets::Activate,
};

#[derive(Message, Clone, Copy)]
pub(super) enum Action {
    Color,
    Move,
    Explore,
    Editor,
    Viewer,
    Connection,
}
#[derive(Component)]
pub(super) struct Status;
#[derive(Component)]
pub(super) struct ExploreLabel;

/// Build semantic collaboration controls shared by native and browser runs.
pub(super) fn setup(
    mut commands: Commands,
    root: Single<Entity, With<dimensify::playground::ExampleChrome>>,
) {
    commands
        .spawn((
            ChildOf(*root),
            Node {
                padding: UiRect::all(px(8)),
                column_gap: px(8),
                row_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
            Hovered::default(),
        ))
        .with_children(|bar| {
            for (action, label) in [
                (Action::Color, "Color"),
                (Action::Move, "Move"),
                (Action::Explore, "Explore"),
                (Action::Editor, "Editor tab"),
                (Action::Viewer, "Viewer tab"),
                (Action::Connection, "Connection"),
            ] {
                let parent = bar.target_entity();
                bar.commands_mut()
                    .spawn_scene(
                        bsn! { @FeathersButton Node { min_width: px(116), height: px(44) } },
                    )
                    .insert(ChildOf(parent))
                    .observe(move |_: On<Activate>, mut actions: MessageWriter<Action>| {
                        actions.write(action);
                    })
                    .with_children(|button| {
                        let mut label = button.spawn((Text::new(label), ThemedText));
                        if matches!(action, Action::Explore) {
                            label.insert(ExploreLabel);
                        }
                    });
            }
        });
    commands.spawn((
        Status,
        Text::new("Connecting to room server..."),
        TextFont::from_font_size(FontSize::Px(14.)),
        ChildOf(*root),
        Node {
            padding: UiRect::all(px(10)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
    ));
}
/// Submit local intents through the existing role-aware session API.
pub(super) fn actions(
    mut actions: MessageReader<Action>,
    options: Res<Options>,
    mut session: ResMut<ClientSession<Cube>>,
    mut demo: ResMut<Demo>,
    mut panel: ResMut<connection::Panel>,
) {
    for action in actions.read() {
        if matches!(action, Action::Connection) {
            panel.open = true;
            continue;
        }
        let Some(room) = &session.snapshot else {
            demo.error = "Connect to a running room server first.".into();
            continue;
        };
        match action {
            Action::Connection => unreachable!(),
            Action::Color | Action::Move => {
                let mut data = room.document.data.clone();
                if matches!(action, Action::Color) {
                    data.color.rotate_left(1);
                } else {
                    data.center[0] = if data.center[0] > 0. { -1. } else { 1. };
                }
                demo.error = session
                    .submit_projection::<CubeState>(data)
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_default();
            }
            Action::Explore => demo.exploring = !demo.exploring,
            Action::Editor | Action::Viewer => {
                if let Some(codes) = session
                    .invitations
                    .as_ref()
                    .and_then(|invites| invites.codes.as_ref())
                {
                    let code = if matches!(action, Action::Editor) {
                        &codes.editor
                    } else {
                        &codes.viewer
                    };
                    open_peer(&options.endpoint, &code.display_code());
                } else {
                    demo.error = "Only the room creator has invitation codes.".into();
                }
            }
        }
    }
}
fn open_peer(endpoint: &str, code: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().unwrap();
        let mut url = url::Url::parse(&window.location().href().unwrap()).unwrap();
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("endpoint", endpoint)
            .append_pair("code", code)
            .append_pair("name", "guest");
        let _ = window.open_with_url_and_target(url.as_str(), "_blank");
    }
    #[cfg(not(target_arch = "wasm32"))]
    println!(
        "Join: cargo run --locked -p dimensify_session --features example-browser --example browser_collaboration -- {endpoint} {code} guest"
    );
}
/// Display acknowledged room state, invitations and actionable connection errors.
pub(super) fn status(
    session: Res<ClientSession<Cube>>,
    demo: Res<Demo>,
    mut labels: Query<&mut Text, With<Status>>,
    mut explore: Query<&mut Text, (With<ExploreLabel>, Without<Status>)>,
) {
    if let Ok(mut label) = explore.single_mut() {
        label.0 = if demo.exploring { "Follow" } else { "Explore" }.into();
    }
    let error = if !demo.error.is_empty() {
        demo.error.clone()
    } else {
        session
            .error
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    let error = if error.is_empty() {
        String::new()
    } else {
        format!("\nError: {error}")
    };
    let text = if let Some(room) = &session.snapshot {
        let me = room
            .participants
            .iter()
            .find(|person| Some(person.id) == session.you);
        let role = me
            .map(|person| format!("{:?}", person.role))
            .unwrap_or_else(|| "Unknown role".into());
        let viewing = if room.presenter == session.you {
            "Presenting"
        } else if demo.exploring {
            "Exploring"
        } else {
            "Following"
        };
        let codes = session
            .invitations
            .as_ref()
            .and_then(|invites| invites.codes.as_ref())
            .map(|codes| {
                format!(
                    "\nEditor {} | Viewer {}",
                    codes.editor.display_code(),
                    codes.viewer.display_code()
                )
            })
            .unwrap_or_default();
        format!(
            "{:?} | {role} | {viewing} | participants={} revision={}{}{error}",
            session.status,
            room.participants.len(),
            room.document.revision,
            codes
        )
    } else {
        format!(
            "{:?}{error}\nUse Connection to choose a host, create/join a room, or retry.",
            session.status
        )
    };
    if let Ok(mut label) = labels.single_mut()
        && label.0 != text
    {
        info!("COLLAB {text}");
        label.0 = text;
    }
}
