use bevy::prelude::*;
use euclid::default::Transform2D;
use lyon::svg::path_utils::build_path;
use lyon::tessellation::StrokeOptions;
use std::{error::Error, fs};
use svgtypes::PathParser;

mod lyon_utils;
// pub use lyon_utils::{build_path, stroke};
pub use lyon_utils::stroke;
mod style;
use style::{StyleSegment, SvgWhole};
pub use style::{StyleStrategy, SvgStyle};

/// Return a zero-cost read-only view of the svg XML document as a graph
fn take_lines_with_style<'a>(doc: &'a roxmltree::Document) -> Vec<(&'a str, &'a str)> {
    doc.descendants()
        .filter(|n| matches!(n.attribute("d"), Some(_)))
        .map(|n| (n.attribute("style").unwrap(), n.attribute("d").unwrap()))
        .collect()
}

/// Parse each "d" node's attribute into a StyleSegment
fn tokenize_svg(path: &str) -> Result<Vec<StyleSegment>, Box<dyn Error>> {
    let xmlfile = fs::read_to_string(path)?;
    let doc = roxmltree::Document::parse(&xmlfile)?;
    Ok(take_lines_with_style(&doc)
        .iter()
        .map(|p| StyleSegment::from(*p))
        .collect())
}

fn max_coords(svg_map: &str) -> (f64, f64) {
    tokenize_svg(svg_map)
        .unwrap()
        .iter()
        .flat_map(|n| PathParser::from(n.traces.as_ref()).map(|n| n.unwrap()))
        .fold((0f64, 0f64), |acc, n| {
            let x_f = match n.x() {
                Some(x) => x.abs().max(acc.0),
                None => acc.0,
            };
            let y_f = match n.y() {
                Some(y) => y.abs().max(acc.1),
                None => acc.1,
            };
            (x_f, y_f)
        })
}

fn max_coords_doc(svg_map: &str) -> (f64, f64) {
    let xmlfile = fs::read_to_string(svg_map).unwrap();
    let doc = roxmltree::Document::parse(&xmlfile).unwrap();
    (
        doc.descendants()
            .filter(|n| n.tag_name().name() == "svg")
            .map(|n| n.attribute("width").unwrap().parse().unwrap())
            .last()
            .unwrap(),
        doc.descendants()
            .filter(|n| n.tag_name().name() == "svg")
            .map(|n| n.attribute("height").unwrap().parse().unwrap())
            .last()
            .unwrap(),
    )
}

/// For each of the paths in a SVG file, apply a StyleStrategy to translate them into entities with
/// functionality added to them, dependent of the SVG properties of the path (stroke, fill...)
pub fn load_svg_map<T: StyleStrategy>(
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    svg_map: &str,
    strategy: T,
) {
    // let wall_thickness = 10.0;
    let (x_max, y_max) = max_coords(svg_map);
    let (x_max, y_max) = (x_max as f32, y_max as f32);

    for StyleSegment { style, traces } in tokenize_svg(svg_map).unwrap().iter() {
        let color_handle = materials.add(strategy.color_decider(style).into());
        // TODO: this transformation are a joke...
        let builder = lyon::path::Path::builder().with_svg().transformed(
            Transform2D::translation(x_max + x_max / 2f32, y_max / 2f32) // translate to bevy coordinates
                .pre_rotate(euclid::Angle::radians(std::f32::consts::PI / 2.)) // rotate 180º for some reason
                .then(&Transform2D::new(0f32, 1f32, 1f32, 0f32, 0f32, 0f32)) // mirror for some reason
                .then_translate(euclid::Vector2D::new(0., -y_max)), // translate again to bevy coordinates
        );
        let path = build_path(builder, traces).unwrap();
        strategy.component_decider(
            &style,
            commands.spawn(stroke(
                path,
                color_handle,
                &mut meshes,
                Vec3::new(-x_max, -y_max, 0.0),
                &StrokeOptions::default()
                    .with_line_width(strategy.width_decider(style))
                    .with_line_cap(strategy.linecap_decider(style))
                    .with_line_join(strategy.linejoin_decider(style)),
            )),
        )
    }
}

/// Load a SVG file as an Entity, return the Commands to allow the user to further modify it
pub fn load_svg(
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    svg_map: &str,
    width: f32,
    height: f32,
    position: Vec2,
) -> Commands {
    let (x_in, y_in) = max_coords_doc(svg_map);
    let (x_max, y_max) = max_coords(svg_map);
    let (x_max, y_max) = (x_max as f32 / 2., y_max as f32 / 2.);
    let (scale_x, scale_y) = ((width / x_in as f32), (height / y_in as f32));

    // let mut sprites = Vec::new();
    let mut tr = Transform::default();
    let mut globe = GlobalTransform::default();
    tr.set_translation(position.extend(0f32));
    globe.set_translation(position.extend(0f32));
    commands.spawn((tr, globe));
    // TODO: this transformation are a joke...
    let parent = commands.current_entity().unwrap();
    for StyleSegment { style, traces } in tokenize_svg(svg_map).unwrap().iter() {
        let builder = lyon::path::Path::builder().with_svg().transformed(
            Transform2D::scale(scale_x, scale_y) // user scale
                .pre_rotate(euclid::Angle::radians(std::f32::consts::PI / 2.)) // rotate 180º for some reason
                .then(&Transform2D::new(0f32, 1f32, 1f32, 0f32, 0f32, 0f32)), // mirror for some reason
        );
        let color_handle = materials.add(SvgWhole.color_decider(style).into());
        let path = build_path(builder, traces).unwrap();
        let sprite = stroke(
            path,
            color_handle,
            &mut meshes,
            Vec3::new(-x_max, -y_max, 0.0),
            &StrokeOptions::default()
                .with_line_width(SvgWhole.width_decider(style))
                .with_line_cap(SvgWhole.linecap_decider(style))
                .with_line_join(SvgWhole.linejoin_decider(style)),
        );
        commands.spawn(sprite);
        let sprite1 = commands.current_entity().unwrap();
        commands.push_children(parent, &[sprite1]);
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use svgtypes::PathSegment;
    #[test]
    fn tokenize_properly() {
        let (_, _) = tokenize_svg("assets/ex.svg")
            .unwrap()
            .iter()
            .flat_map(|n| PathParser::from(n.traces.as_ref()).map(|n| n.unwrap()))
            .fold((0f64, 0f64), |acc, n| match n {
                PathSegment::MoveTo { abs: _, x, y } => (x.abs().max(acc.0), y.abs().max(acc.1)),
                PathSegment::HorizontalLineTo { abs: _, x } => (x.abs().max(acc.0), acc.1),
                PathSegment::VerticalLineTo { abs: _, y } => (acc.0, y.abs().max(acc.1)),
                _ => {
                    println!("Found a not yet handled PathSegment");
                    acc
                }
            });
    }
}
