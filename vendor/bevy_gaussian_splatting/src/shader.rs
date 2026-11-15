//! Stable WESL package paths for shaders retained under internal handles.
use bevy::shader::Shader;
use std::borrow::Cow;

pub(crate) fn from_wesl(source: impl Into<Cow<'static, str>>, path: impl Into<String>) -> Shader {
    let path = path.into().replace('\\', "/");
    let relative = path
        .rsplit_once("/src/")
        .map(|(_, relative)| relative)
        .unwrap_or_else(|| path.strip_prefix("src/").unwrap_or(&path));
    Shader::from_wesl(
        source,
        format!("embedded://bevy_gaussian_splatting/{relative}"),
    )
}

#[cfg(test)]
pub(crate) fn load_shader_tree(
    path: &std::path::Path,
    assets: &mut bevy::prelude::Assets<Shader>,
    handles: &mut Vec<bevy::prelude::Handle<Shader>>,
) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            load_shader_tree(&path, assets, handles);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "wesl")
        {
            let source = std::fs::read_to_string(&path).unwrap();
            let path = path.to_str().unwrap().replace('\\', "/");
            let (package, relative) = path.rsplit_once("/src/").unwrap();
            let package = package
                .rsplit('/')
                .next()
                .unwrap()
                .split('-')
                .next()
                .unwrap();
            handles.push(assets.add(Shader::from_wesl(
                source,
                format!("embedded://{package}/{relative}"),
            )));
        }
    }
}
