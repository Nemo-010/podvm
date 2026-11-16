/// `reference:path` where the reference may carry a registry, a tag, or
/// both. Slashes allowed, unlike a container name: this is the image half
/// of addressing, and `copy_named` tries the container half first.
pub(super) fn split_image(arg: &str) -> Option<(String, String)> {
    let (image, path) = arg.rsplit_once(':')?;
    if image.is_empty() || path.is_empty() {
        return None;
    }
    Some((image.to_string(), path.to_string()))
}
