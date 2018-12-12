use gfx;
use gfx::format::I8Norm;

use m3d;
use render::{
    DebugPos, ObjectVertex, ShapeVertex, ShapePolygon,
};

use std::fs::File;
use std::ops::Range;


#[derive(Clone)]
pub struct Mesh<R: gfx::Resources> {
    pub slice: gfx::Slice<R>,
    pub buffer: gfx::handle::Buffer<R, ObjectVertex>,
    pub offset: [f32; 3],
    pub bbox: ([f32; 3], [f32; 3], f32),
    pub physics: m3d::Physics,
}

#[derive(Clone, Debug)]
pub struct Polygon {
    pub middle: [f32; 3],
    pub normal: [f32; 3],
    pub samples: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct Shape<R: gfx::Resources> {
    pub polygons: Vec<Polygon>,
    pub samples: Vec<RawVertex>,
    pub vertex_buf: gfx::handle::Buffer<R, ShapeVertex>,
    pub vertex_view: gfx::handle::ShaderResourceView<R, [f32; 4]>,
    pub polygon_buf: gfx::handle::Buffer<R, ShapePolygon>,
    pub sample_buf: Option<gfx::handle::Buffer<R, DebugPos>>,
    pub bounds: m3d::Bounds,
}

impl<R: gfx::Resources> Shape<R> {
    pub fn make_draw_slice(&self) -> gfx::Slice<R> {
        gfx::Slice {
            start: 0,
            end: 4,
            base_vertex: 0,
            instances: Some((self.polygons.len() as _, 0)),
            buffer: gfx::IndexBuffer::Auto,
        }
    }
}

pub type RawVertex = [i8; 3];

struct Tessellator {
    samples: Vec<RawVertex>,
}

impl Tessellator {
    fn new() -> Self {
        Tessellator {
            samples: Vec::new(),
        }
    }
    fn tessellate(
        &mut self,
        corners: &[RawVertex],
        _middle: RawVertex,
    ) -> &[RawVertex] {
        let go_deeper = false;
        self.samples.clear();
        //self.samples.push(middle);
        let mid_sum = corners
            .iter()
            .fold([0f32; 3], |sum, cur| [
                sum[0] + cur[0] as f32,
                sum[1] + cur[1] as f32,
                sum[2] + cur[2] as f32,
            ]);
        if go_deeper {
            let corner_ratio = 0.66f32;
            let div = (1.0 - corner_ratio) / corners.len() as f32;
            let mid_rationed = [
                (mid_sum[0] * div) as i8,
                (mid_sum[1] * div) as i8,
                (mid_sum[2] * div) as i8,
            ];
            let ring1 = corners.iter().map(|c| {
                [
                    (corner_ratio * c[0] as f32) as i8 + mid_rationed[0],
                    (corner_ratio * c[1] as f32) as i8 + mid_rationed[1],
                    (corner_ratio * c[2] as f32) as i8 + mid_rationed[2],
                ]
            }).collect::<Vec<_>>();
            self.samples.extend((0 .. corners.len()).map(|i| {
                let c0 = &ring1[i];
                let c1 = &ring1[(i+1)%corners.len()];
                [
                    c0[0] / 2 + c1[0] / 2,
                    c0[1] / 2 + c1[1] / 2,
                    c0[2] / 2 + c1[2] / 2,
                ]
            }));
            self.samples.extend(ring1);
            self.samples.push([
                (mid_sum[0] / corners.len() as f32) as i8,
                (mid_sum[1] / corners.len() as f32) as i8,
                (mid_sum[2] / corners.len() as f32) as i8,
            ]);
        } else {
            let div = 0.5 / corners.len() as f32;
            let mid_half = [
                (mid_sum[0] * div) as i8,
                (mid_sum[1] * div) as i8,
                (mid_sum[2] * div) as i8,
            ];
            self.samples.extend(corners.iter().map(|c| {
                [
                    c[0] / 2 + mid_half[0],
                    c[1] / 2 + mid_half[1],
                    c[2] / 2 + mid_half[2],
                ]
            }));
        }
        &self.samples
    }
}


fn vec_i2f(v: [i32; 3]) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

pub fn load_c3d<R, F>(
    raw: m3d::Mesh<m3d::Geometry<m3d::DrawTriangle>>,
    factory: &mut F,
) -> Mesh<R>
where
    R: gfx::Resources,
    F: gfx::traits::FactoryExt<R>,
{
    let positions = &raw.geometry.positions;
    let normals = &raw.geometry.normals;
    let vertices = raw.geometry.polygons
        .iter()
        .flat_map(|tri| {
            tri.vertices.into_iter().map(move |v| {
                let p = positions[v.pos as usize];
                let n = normals[v.normal as usize];
                ObjectVertex {
                    pos: [p[0], p[1], p[2], 1],
                    color: tri.material[0],
                    normal: [I8Norm(n[0]), I8Norm(n[1]), I8Norm(n[2]), I8Norm(0)],
                }
            })
        })
        .collect::<Vec<_>>();

    let (buffer, slice) = factory.create_vertex_buffer_with_slice(&vertices, ());

    debug!("\tGot {} GPU vertices...", vertices.len());
    Mesh {
        slice,
        buffer,
        offset: vec_i2f(raw.parent_off),
        bbox: (
            vec_i2f(raw.bounds.coord_min),
            vec_i2f(raw.bounds.coord_max),
            raw.max_radius as f32,
        ),
        physics: raw.physics,
    }
}

pub fn load_c3d_shape<R, F>(
    raw: m3d::Mesh<m3d::Geometry<m3d::CollisionQuad>>,
    factory: &mut F,
    with_sample_buf: bool,
) -> Shape<R>
where
    R: gfx::Resources,
    F: gfx::traits::FactoryExt<R>,
{
    debug!("\tTessellating polygons...");
    let mut polygons = Vec::new();
    let mut polygon_data = Vec::with_capacity(raw.geometry.polygons.len());
    let mut samples = Vec::new();
    let mut sample_data = Vec::new();
    let mut tess = Tessellator::new();

    for quad in &raw.geometry.polygons {
        let corners = [
            raw.geometry.positions[quad.vertices[0] as usize],
            raw.geometry.positions[quad.vertices[1] as usize],
            raw.geometry.positions[quad.vertices[2] as usize],
            raw.geometry.positions[quad.vertices[3] as usize],
        ];
        let square = 1.0; //TODO: compute polygon square
        let middle = [
            quad.middle[0] as f32,
            quad.middle[1] as f32,
            quad.middle[2] as f32,
        ];
        polygon_data.push(ShapePolygon {
            indices: quad.vertices,
            normal: [
                I8Norm(quad.flat_normal[0]),
                I8Norm(quad.flat_normal[1]),
                I8Norm(quad.flat_normal[2]),
                I8Norm(0),
            ],
            origin_square: [ middle[0], middle[1], middle[2], square ],
        });
        let normal = [
            quad.flat_normal[0] as f32 / m3d::NORMALIZER,
            quad.flat_normal[1] as f32 / m3d::NORMALIZER,
            quad.flat_normal[2] as f32 / m3d::NORMALIZER,
        ];
        let cur_samples = tess.tessellate(&corners[..], quad.middle);

        if with_sample_buf {
            let mut nlen = 16.0;
            sample_data.push(DebugPos {
                pos: [ middle[0], middle[1], middle[2], 1.0],
            });
            sample_data.push(DebugPos {
                pos: [
                    middle[0] + quad.flat_normal[0] as f32 * nlen,
                    middle[1] + quad.flat_normal[1] as f32 * nlen,
                    middle[2] + quad.flat_normal[2] as f32 * nlen,
                    1.0,
                ],
            });
            nlen = 4.0;
            for s in cur_samples {
                sample_data.push(DebugPos {
                    pos: [s[0] as f32, s[1] as f32, s[2] as f32, 1.0],
                });
                sample_data.push(DebugPos {
                    pos: [
                        s[0] as f32 + quad.flat_normal[0] as f32 * nlen,
                        s[1] as f32 + quad.flat_normal[1] as f32 * nlen,
                        s[2] as f32 + quad.flat_normal[2] as f32 * nlen,
                        1.0,
                    ],
                });
            }
        }

        polygons.push(Polygon {
            middle,
            normal,
            samples: samples.len() .. samples.len() + cur_samples.len(),
        });
        samples.extend(cur_samples);
    }

    let vertices = raw.geometry.positions
        .into_iter()
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32, 1.0])
        .collect::<Vec<_>>();
    let vertex_buf = factory
        .create_buffer_immutable(
            &vertices,
            gfx::buffer::Role::Vertex,
            gfx::memory::Bind::SHADER_RESOURCE,
        )
        .unwrap();

    Shape {
        polygons,
        samples,
        vertex_view: factory
            .view_buffer_as_shader_resource(&vertex_buf)
            .unwrap(),
        vertex_buf,
        polygon_buf: factory.create_vertex_buffer(&polygon_data),
        sample_buf: if with_sample_buf {
            Some(factory.create_vertex_buffer(&sample_data))
        } else {
            None
        },
        bounds: raw.bounds,
    }
}

//TODO: convert to use m3d::Model as a source

pub type RenderModel<R> = m3d::Model<Mesh<R>, Shape<R>>;

pub fn load_m3d<R, F>(
    file: File, factory: &mut F
) -> RenderModel<R>
where
    R: gfx::Resources,
    F: gfx::traits::FactoryExt<R>,
{
    let raw = m3d::FullModel::load(file);

    RenderModel {
        body: load_c3d(raw.body, factory),
        shape: load_c3d_shape(raw.shape, factory, true),
        dimensions: raw.dimensions,
        max_radius: raw.max_radius,
        color: raw.color,
        wheels: raw.wheels
            .into_iter()
            .map(|wheel| wheel.map(|mesh| {
                load_c3d(mesh, factory)
            }))
            .collect(),
        debris: raw.debris
            .into_iter()
            .map(|debrie| m3d::Debrie {
                mesh: load_c3d(debrie.mesh, factory),
                shape: load_c3d_shape(debrie.shape, factory, false),
            })
            .collect(),
        slots: m3d::Slot::map_all(raw.slots, |_, _| unreachable!()),
    }
}
