
/// if there's package prefix, strip it, otherwise return the base_dir + original string
pub fn replace_package_with_base_dir<P>(filename: &str, base_dir: &Option<P>) -> String
where
    P: std::fmt::Display,
{
    match filename.strip_prefix("package://") {
        Some(path) => path.to_string(),
        None => match base_dir {
            Some(base_dir) => {
                format!("{base_dir}/{filename}")
            }
            None => filename.to_string(),
        },
    }
}