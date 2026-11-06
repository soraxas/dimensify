//! URL loading with an in-memory transport, plus scene replacement.
use super::{fetch::Reply, fetch::Transport, *};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use url::Url;

const TRIANGLE: &str = "solid t\nfacet normal 0 0 0\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";

/// Two links share one package Collada mesh; a third uses a relative STL.
const ROBOT: &str = r#"<robot name="url_arm">
  <link name="base"><visual><geometry><mesh filename="package://arm_description/meshes/rotated.dae"/></geometry></visual></link>
  <link name="arm"><visual><geometry><mesh filename="package://arm_description/meshes/rotated.dae"/></geometry></visual></link>
  <link name="tip"><visual><geometry><mesh filename="../meshes/tip.stl"/></geometry></visual></link>
  <joint name="hinge" type="revolute">
    <parent link="base"/><child link="arm"/><origin xyz="0 0 1"/>
    <axis xyz="0 0 1"/><limit lower="-1" upper="1" effort="1" velocity="1"/>
  </joint>
  <joint name="tool" type="fixed"><parent link="arm"/><child link="tip"/></joint>
</robot>"#;

fn collada() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rotated.dae")).unwrap()
}

/// Serves fixed bodies, records every request and its byte limit, and can hold replies.
#[derive(Default)]
struct Memory {
    files: HashMap<String, Vec<u8>>,
    requests: Mutex<Vec<(String, usize)>>,
    held: Mutex<Vec<Reply>>,
    hold: bool,
}

impl Transport for Memory {
    fn get(&self, url: &Url, limit: usize, reply: Reply) {
        self.requests.lock().unwrap().push((url.to_string(), limit));
        if self.hold {
            self.held.lock().unwrap().push(reply);
            return;
        }
        reply(match self.files.get(url.as_str()) {
            Some(body) if body.len() <= limit => Ok(body.clone()),
            Some(_) => Err("too large".into()),
            None => Err("HTTP 404".into()),
        });
    }
}

fn serve(files: &[(&str, Vec<u8>)]) -> Arc<Memory> {
    Arc::new(Memory {
        files: files
            .iter()
            .map(|(url, body)| (url.to_string(), body.clone()))
            .collect(),
        ..Default::default()
    })
}

fn finish(transport: Arc<Memory>, url: &str, packages: Vec<(String, Url)>) -> Result<RobotModel> {
    let mut load = RobotLoad::with_transport(transport, Url::parse(url).unwrap(), packages);
    for _ in 0..100 {
        if let Some(result) = load.poll() {
            assert!(load.poll().is_none(), "a load reports its result once");
            return result;
        }
    }
    panic!("load did not finish");
}

fn requested(transport: &Memory) -> Vec<String> {
    transport
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(url, _)| url.clone())
        .collect()
}

fn mesh_count(model: &RobotModel) -> usize {
    model.visuals.iter().map(Vec::len).sum()
}

#[test]
fn ros_layout_downloads_each_unique_mesh_once() {
    let base = "https://host.test/repo/arm_description";
    let transport = serve(&[
        (&format!("{base}/urdf/arm.urdf"), ROBOT.into()),
        (&format!("{base}/meshes/rotated.dae"), collada()),
        (&format!("{base}/meshes/tip.stl"), TRIANGLE.into()),
    ]);
    let model = finish(transport.clone(), &format!("{base}/urdf/arm.urdf"), vec![]).unwrap();
    assert_eq!(model.name(), "url_arm");
    assert_eq!(model.controllable_joints().count(), 1);
    assert_eq!(mesh_count(&model), 3);
    // One URDF plus two unique meshes; the matching package directory needs no probing.
    assert_eq!(requested(&transport).len(), 3);
    let limits: Vec<_> = transport
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(_, limit)| *limit)
        .collect();
    assert_eq!(limits, vec![8 << 20, 64 << 20, 64 << 20]);
}

#[test]
fn url_and_disk_loads_build_identical_models() {
    let directory = std::env::temp_dir().join(format!("robosim-url-eq-{}", std::process::id()));
    let package = directory.join("arm_description");
    std::fs::create_dir_all(package.join("urdf")).unwrap();
    std::fs::create_dir_all(package.join("meshes")).unwrap();
    std::fs::write(package.join("meshes/rotated.dae"), collada()).unwrap();
    std::fs::write(package.join("meshes/tip.stl"), TRIANGLE).unwrap();
    let disk = RobotModel::from_urdf(ROBOT, &package.join("urdf")).unwrap();
    std::fs::remove_dir_all(directory).unwrap();

    let base = "https://host.test/arm_description";
    let url = finish(
        serve(&[
            (&format!("{base}/urdf/arm.urdf"), ROBOT.into()),
            (&format!("{base}/meshes/rotated.dae"), collada()),
            (&format!("{base}/meshes/tip.stl"), TRIANGLE.into()),
        ]),
        &format!("{base}/urdf/arm.urdf"),
        vec![],
    )
    .unwrap();
    let shape = |model: &RobotModel| -> Vec<Vec<(usize, Transform)>> {
        model
            .visuals
            .iter()
            .map(|parts| {
                parts
                    .iter()
                    .map(|part| (part.mesh.count_vertices(), part.transform))
                    .collect()
            })
            .collect()
    };
    assert_eq!(shape(&disk), shape(&url));
}

#[test]
fn unmatched_packages_probe_ancestors_and_reuse_the_proven_base() {
    let two = ROBOT.replace(
        "../meshes/tip.stl",
        "package://arm_description/meshes/tip.stl",
    );
    let transport = serve(&[
        ("https://host.test/a/b/robots/arm.urdf", two.into()),
        (
            "https://host.test/a/arm_description/meshes/rotated.dae",
            collada(),
        ),
        (
            "https://host.test/a/arm_description/meshes/tip.stl",
            TRIANGLE.into(),
        ),
    ]);
    let model = finish(
        transport.clone(),
        "https://host.test/a/b/robots/arm.urdf",
        vec![],
    )
    .unwrap();
    assert_eq!(mesh_count(&model), 3);
    let requests = requested(&transport);
    // Both meshes start together and probe robots/, b/, then a/, deepest first.
    for directory in ["a/b/robots", "a/b", "a"] {
        assert!(
            requests.contains(&format!(
                "https://host.test/{directory}/arm_description/meshes/rotated.dae"
            )),
            "{requests:?}"
        );
    }
    assert!(
        !requests
            .iter()
            .any(|r| r == "https://host.test/arm_description/meshes/rotated.dae"),
        "probing stops at the first hit: {requests:?}"
    );
}

#[test]
fn files_started_after_a_hit_reuse_the_proven_package_base() {
    // Eight unique meshes exceed the six concurrent requests, so the last two
    // start only after the first wave has located the package two levels up.
    let links: String = (0..8)
        .map(|i| format!(r#"<link name="l{i}"><visual><geometry><mesh filename="package://arm_description/m{i}.stl"/></geometry></visual></link>"#))
        .collect();
    let joints: String = (1..8)
        .map(|i| format!(r#"<joint name="j{i}" type="fixed"><parent link="l{}"/><child link="l{i}"/></joint>"#, i - 1))
        .collect();
    let urdf = format!(r#"<robot name="eight">{links}{joints}</robot>"#);
    let mut files = vec![(
        "https://host.test/a/b/robots/arm.urdf".to_owned(),
        urdf.into_bytes(),
    )];
    for i in 0..8 {
        files.push((
            format!("https://host.test/a/arm_description/m{i}.stl"),
            TRIANGLE.into(),
        ));
    }
    let files: Vec<_> = files.iter().map(|(u, b)| (u.as_str(), b.clone())).collect();
    let transport = serve(&files);
    let model = finish(
        transport.clone(),
        "https://host.test/a/b/robots/arm.urdf",
        vec![],
    )
    .unwrap();
    assert_eq!(mesh_count(&model), 8);
    let requests = requested(&transport);
    for i in 0..6 {
        let attempts = requests
            .iter()
            .filter(|r| r.ends_with(&format!("/m{i}.stl")))
            .count();
        assert_eq!(attempts, 3, "first wave probes robots/, b/, then a/");
    }
    for i in 6..8 {
        let attempts: Vec<_> = requests
            .iter()
            .filter(|r| r.ends_with(&format!("/m{i}.stl")))
            .collect();
        assert_eq!(
            attempts,
            vec![&format!("https://host.test/a/arm_description/m{i}.stl")],
            "later files go straight to the proven base"
        );
    }
}

#[test]
fn package_overrides_skip_probing() {
    let transport = serve(&[
        ("https://host.test/x/arm.urdf", ROBOT.into()),
        ("https://cdn.test/pkg/meshes/rotated.dae", collada()),
        ("https://host.test/meshes/tip.stl", TRIANGLE.into()),
    ]);
    let source = RobotSource::parse("https://host.test/x/arm.urdf")
        .unwrap()
        .with_package("arm_description=https://cdn.test/pkg")
        .unwrap();
    let RobotSource::Url { url, packages } = source else {
        unreachable!()
    };
    let model = finish(transport.clone(), url.as_str(), packages).unwrap();
    assert_eq!(mesh_count(&model), 3);
    assert_eq!(requested(&transport).len(), 3);
}

#[test]
fn missing_meshes_and_bad_urdfs_fail_with_actionable_errors() {
    let missing = finish(
        serve(&[(
            "https://host.test/x/arm.urdf",
            ROBOT
                .replace(
                    "../meshes/tip.stl",
                    "package://arm_description/meshes/tip.stl",
                )
                .into(),
        )]),
        "https://host.test/x/arm.urdf",
        vec![],
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        missing.contains("package=arm_description=<base URL>"),
        "{missing}"
    );
    let relative = finish(
        serve(&[("https://host.test/x/arm.urdf", ROBOT.into())]),
        "https://host.test/x/arm.urdf",
        vec![],
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        relative.contains("https://host.test/meshes/tip.stl"),
        "relative misses name the resolved URL: {relative}"
    );

    let invalid = finish(
        serve(&[(
            "https://host.test/x/arm.urdf",
            b"<robot name='x'></robot>".to_vec(),
        )]),
        "https://host.test/x/arm.urdf",
        vec![],
    );
    assert!(invalid.is_err());

    let absent = finish(serve(&[]), "https://host.test/none.urdf", vec![]);
    assert!(absent.err().unwrap().to_string().contains("HTTP 404"));

    let oversized = finish(
        serve(&[("https://host.test/big.urdf", vec![b' '; (8 << 20) + 1])]),
        "https://host.test/big.urdf",
        vec![],
    );
    assert!(oversized.err().unwrap().to_string().contains("too large"));
}

#[test]
fn dropped_loads_ignore_late_replies() {
    let transport = Arc::new(Memory {
        hold: true,
        ..Default::default()
    });
    let mut load = RobotLoad::with_transport(
        transport.clone(),
        Url::parse("https://host.test/x/arm.urdf").unwrap(),
        vec![],
    );
    assert!(load.poll().is_none());
    assert_eq!(load.progress(), None);
    drop(load);
    for reply in transport.held.lock().unwrap().drain(..) {
        reply(Ok(ROBOT.into()));
    }
}

#[test]
fn inserting_a_new_model_replaces_the_spawned_robot() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins(RobotPlugin);
    app.update();
    let roots = |app: &mut App| -> Vec<String> {
        app.world_mut()
            .query_filtered::<&Name, With<RobotRoot>>()
            .iter(app.world())
            .map(|name| name.to_string())
            .collect()
    };
    assert!(roots(&mut app).is_empty(), "no model, no robot");

    let first = RobotModel::from_urdf(DEMO_URDF, Path::new(".")).unwrap();
    let first_name = first.name().to_owned();
    app.insert_resource(first);
    app.update();
    let meshes = app.world_mut().query::<&Mesh3d>().iter(app.world()).count();
    assert_eq!(roots(&mut app), vec![first_name]);

    let second = finish(
        serve(&[
            (
                "https://host.test/arm_description/urdf/arm.urdf",
                ROBOT.into(),
            ),
            (
                "https://host.test/arm_description/meshes/rotated.dae",
                collada(),
            ),
            (
                "https://host.test/arm_description/meshes/tip.stl",
                TRIANGLE.into(),
            ),
        ]),
        "https://host.test/arm_description/urdf/arm.urdf",
        vec![],
    )
    .unwrap();
    app.insert_resource(second);
    app.update();
    assert_eq!(roots(&mut app), vec!["url_arm".to_owned()]);
    let after = app.world_mut().query::<&Mesh3d>().iter(app.world()).count();
    assert_eq!(after, 3, "the demo's {meshes} meshes were despawned");
    // Ticking again does not respawn the same model.
    app.update();
    assert_eq!(roots(&mut app).len(), 1);
}

#[test]
fn source_parsing_distinguishes_files_and_urls() {
    assert_eq!(
        RobotSource::parse(" robot.urdf ").unwrap(),
        RobotSource::File(PathBuf::from("robot.urdf"))
    );
    assert!(matches!(
        RobotSource::parse("https://github.com/o/r/blob/main/robot.urdf").unwrap(),
        RobotSource::Url { url, .. } if url.host_str() == Some("raw.githubusercontent.com")
    ));
}
