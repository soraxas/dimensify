//! Ordinary PLY vertices, independent of the viewer and Gaussian loaders.
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use ply_rs::{
    parser::Parser,
    ply::{Encoding, Property, PropertyAccess, PropertyType, ScalarType},
};
use std::io::Cursor;

#[derive(Default)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    normal: [f32; 3],
}
impl PropertyAccess for Vertex {
    fn new() -> Self {
        Self {
            color: [1.; 4],
            ..default()
        }
    }
    fn set_property(&mut self, name: String, property: Property) {
        let (value, scale) = match property {
            Property::Char(v) => (v as f32, i8::MAX as f32),
            Property::UChar(v) => (v as f32, u8::MAX as f32),
            Property::Short(v) => (v as f32, i16::MAX as f32),
            Property::UShort(v) => (v as f32, u16::MAX as f32),
            Property::Int(v) => (v as f32, i32::MAX as f32),
            Property::UInt(v) => (v as f32, u32::MAX as f32),
            Property::Float(v) => (v, 1.),
            Property::Double(v) => (v as f32, 1.),
            _ => return,
        };
        match name.as_str() {
            "x" => self.position[0] = value,
            "y" => self.position[1] = value,
            "z" => self.position[2] = value,
            "nx" => self.normal[0] = value,
            "ny" => self.normal[1] = value,
            "nz" => self.normal[2] = value,
            "red" | "diffuse_red" => self.color[0] = (value / scale).clamp(0., 1.),
            "green" | "diffuse_green" => self.color[1] = (value / scale).clamp(0., 1.),
            "blue" | "diffuse_blue" => self.color[2] = (value / scale).clamp(0., 1.),
            "alpha" => self.color[3] = (value / scale).clamp(0., 1.),
            _ => {}
        }
    }
}

/// Decode ASCII or either binary PLY encoding. Faces are ignored; vertices
/// must precede other nonempty elements. RGB and normals are optional.
pub fn decode_ply(bytes: &[u8]) -> Result<Mesh, String> {
    // ply-rs parses header numbers with unchecked conversions. Validate those
    // fields before its grammar, and never reserve from an unbounded count.
    let end = bytes
        .split_inclusive(|&b| b == b'\n')
        .scan(0, |n, line| {
            *n += line.len();
            Some((*n, line))
        })
        .find(|(_, line)| line.trim_ascii() == b"end_header")
        .map(|(n, _)| n)
        .ok_or("PLY is missing end_header")?;
    let header_text = std::str::from_utf8(&bytes[..end]).map_err(|_| "Invalid PLY header")?;
    for line in header_text.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields.first() == Some(&"element") {
            fields
                .get(2)
                .ok_or("Invalid PLY element")?
                .parse::<usize>()
                .map_err(|_| "Invalid PLY element count")?;
        }
        if fields.first() == Some(&"format") {
            let (major, minor) = fields
                .get(2)
                .ok_or("Invalid PLY version")?
                .split_once('.')
                .ok_or("Invalid PLY version")?;
            major.parse::<u16>().map_err(|_| "Invalid PLY version")?;
            minor.parse::<u8>().map_err(|_| "Invalid PLY version")?;
        }
    }
    let parser = Parser::<Vertex>::new();
    let mut reader = Cursor::new(bytes);
    let header = parser
        .read_header(&mut reader)
        .map_err(|e| format!("Invalid PLY header: {e}"))?;
    let vertex = header
        .elements
        .get("vertex")
        .ok_or("PLY is missing vertex elements")?;
    if vertex.count == 0 {
        return Err("PLY has no points".into());
    }
    for (name, element) in &header.elements {
        if name == "vertex" {
            break;
        }
        if element.count > 0 {
            return Err("PLY vertices must precede other elements".into());
        }
    }
    for name in ["x", "y", "z"] {
        if !vertex.properties.contains_key(name) {
            return Err(format!("PLY is missing {name}"));
        }
    }
    if vertex
        .properties
        .values()
        .any(|p| !matches!(p.data_type, PropertyType::Scalar(_)))
    {
        return Err("PLY vertex list properties are not supported".into());
    }
    let stride = vertex
        .properties
        .values()
        .map(|p| match p.data_type {
            PropertyType::Scalar(ScalarType::Char | ScalarType::UChar) => 1,
            PropertyType::Scalar(ScalarType::Short | ScalarType::UShort) => 2,
            PropertyType::Scalar(ScalarType::Int | ScalarType::UInt | ScalarType::Float) => 4,
            PropertyType::Scalar(ScalarType::Double) => 8,
            _ => 0,
        })
        .sum::<usize>();
    let minimum = if header.encoding == Encoding::Ascii {
        vertex.properties.len()
    } else {
        stride
    };
    if vertex
        .count
        .checked_mul(minimum)
        .is_none_or(|n| n > bytes.len().saturating_sub(end))
    {
        return Err("PLY payload is too small for its vertex count".into());
    }
    let vertices = if header.encoding == Encoding::Ascii {
        // ply-rs' ASCII grammar rejects valid forms such as .5 and 1E+00.
        // Rust's scalar parsers accept the full numeric syntax and check bounds.
        let text = std::str::from_utf8(&bytes[end..]).map_err(|_| "Invalid ASCII PLY payload")?;
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let mut vertices = Vec::new();
        vertices
            .try_reserve_exact(vertex.count)
            .map_err(|_| "Not enough memory for PLY vertices")?;
        for _ in 0..vertex.count {
            let mut tokens = lines
                .next()
                .ok_or("Truncated PLY vertex")?
                .split_ascii_whitespace();
            let mut point = Vertex::new();
            for (name, property) in &vertex.properties {
                let token = tokens.next().ok_or("Truncated PLY vertex")?;
                let value = ascii_property(token, &property.data_type)?;
                point.set_property(name.clone(), value);
            }
            if tokens.next().is_some() {
                return Err("PLY vertex has more values than declared properties".into());
            }
            vertices.push(point);
        }
        vertices
    } else {
        parser
            .read_payload_for_element(&mut reader, vertex, &header)
            .map_err(|e| format!("Invalid PLY vertices: {e}"))?
    };
    if vertices.iter().any(|v| {
        v.position
            .iter()
            .chain(&v.color)
            .chain(&v.normal)
            .any(|x| !x.is_finite())
    }) {
        return Err("PLY coordinates and attributes must be finite".into());
    }
    let mut mesh = Mesh::new(PrimitiveTopology::PointList, RenderAssetUsages::default());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices.iter().map(|v| v.position).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vertices.iter().map(|v| v.color).collect::<Vec<_>>(),
    );
    if ["nx", "ny", "nz"]
        .iter()
        .all(|n| vertex.properties.contains_key(*n))
    {
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vertices
                .iter()
                .map(|v| {
                    Vec3::from(v.normal)
                        .try_normalize()
                        .unwrap_or(Vec3::Y)
                        .to_array()
                })
                .collect::<Vec<_>>(),
        );
    }
    Ok(mesh)
}

fn ascii_property(token: &str, data_type: &PropertyType) -> Result<Property, String> {
    macro_rules! scalar {
        ($t:ty,$variant:ident) => {
            token
                .parse::<$t>()
                .map(Property::$variant)
                .map_err(|_| format!("Invalid PLY scalar: {token}"))
        };
    }
    match data_type {
        PropertyType::Scalar(ScalarType::Char) => scalar!(i8, Char),
        PropertyType::Scalar(ScalarType::UChar) => scalar!(u8, UChar),
        PropertyType::Scalar(ScalarType::Short) => scalar!(i16, Short),
        PropertyType::Scalar(ScalarType::UShort) => scalar!(u16, UShort),
        PropertyType::Scalar(ScalarType::Int) => scalar!(i32, Int),
        PropertyType::Scalar(ScalarType::UInt) => scalar!(u32, UInt),
        PropertyType::Scalar(ScalarType::Float) => scalar!(f32, Float),
        PropertyType::Scalar(ScalarType::Double) => scalar!(f64, Double),
        _ => Err("PLY vertex list properties are not supported".into()),
    }
}

// Parsing runs inside the renderer's asynchronous loader, rather than Update.
pub(super) struct PlyLoader<S> {
    mesh: bevy_pointcloud::PointCloudMeshLoader,
    source: std::marker::PhantomData<S>,
}
impl<S: bevy_pointcloud::ByteSource> bevy_pointcloud::OctreeLoader for PlyLoader<S> {
    type Source = S;
    type Hierarchy = ();
    type Error = std::io::Error;
    type Settings = ();
    async fn from_source(source: S, _: ()) -> Result<Self, Self::Error> {
        let bytes = source.read_to_end(0).await.map_err(std::io::Error::other)?;
        let mesh = decode_ply(&bytes).map_err(std::io::Error::other)?;
        Ok(Self {
            mesh: mesh.into(),
            source: std::marker::PhantomData,
        })
    }
    async fn load_initial_hierarchy(
        &self,
        builder: &mut bevy_pointcloud::OctreeHierarchyBuilder<()>,
    ) -> Result<(), Self::Error> {
        self.mesh
            .load_initial_hierarchy(builder)
            .await
            .map_err(std::io::Error::other)
    }
    async fn load_chunk(&self, node: &()) -> Result<bevy_pointcloud::ChunkLoadResult, Self::Error> {
        self.mesh
            .load_chunk(node)
            .await
            .map_err(std::io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn ascii_vertices_keep_positions_colors_and_normals_and_ignore_faces() {
        let mesh = decode_ply(b"ply\nformat ascii 1.0\nelement vertex 2\nproperty double x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nproperty float nx\nproperty float ny\nproperty float nz\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n1 2 3 255 128 0 0 0 1\n4 5 6 0 255 255 0 1 0\n3 0 1 0\n").unwrap();
        assert_eq!(mesh.count_vertices(), 2);
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert_eq!(p, &[[1., 2., 3.], [4., 5., 6.]]);
        let Some(VertexAttributeValues::Float32x4(c)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors")
        };
        assert!((c[0][1] - 128. / 255.).abs() < 0.0001);
        assert_eq!(c[1], [0., 1., 1., 1.]);
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        assert_eq!(n, &[[0., 0., 1.], [0., 1., 0.]]);
    }

    #[test]
    fn binary_vertices_support_both_endiannesses_and_optional_attributes() {
        for (encoding, little) in [("binary_little_endian", true), ("binary_big_endian", false)] {
            let mut bytes = format!("ply\nformat {encoding} 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n").into_bytes();
            for v in [1.25_f32, -2.5, 3.] {
                bytes.extend(if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                });
            }
            let mesh = decode_ply(&bytes).unwrap();
            let Some(VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions")
            };
            assert_eq!(p, &[[1.25, -2.5, 3.]]);
            assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_none());
        }
    }

    #[test]
    fn ascii_accepts_common_scientific_and_fraction_notation() {
        let mesh = decode_ply(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n1E+00 .5 1.\n").unwrap();
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert_eq!(p, &[[1., 0.5, 1.]]);
    }

    #[test]
    fn zero_length_normals_have_a_finite_unit_fallback() {
        let mesh = decode_ply(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nproperty float nx\nproperty float ny\nproperty float nz\nend_header\n1 2 3 0 0 0\n").unwrap();
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        let normal = Vec3::from(normals[0]);
        assert!(normal.is_finite() && (normal.length() - 1.).abs() < 0.0001);
    }
    #[test]
    fn truncated_vertex_cannot_consume_following_face_records() {
        assert!(decode_ply(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n1 2\n3 0 0 0\n").is_err());
    }

    #[test]
    fn invalid_or_truncated_ply_is_an_error() {
        for bytes in [
            &b"not ply"[..],
            &b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nend_header\n1\n"[..],
            &b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\nnan 1 2\n"[..],
            &b"ply\nformat binary_little_endian 1.0\nelement vertex 999999999\nproperty float x\nproperty float y\nproperty float z\nend_header\n"[..],
        ] { assert!(decode_ply(bytes).is_err()); }
    }
}
