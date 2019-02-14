//! Drawing primitives used for the custom widgets


use cairo;

use crate::util::{Point, Size, Rect};

pub fn draw_selection_marker(context: &cairo::Context, center: Point) {
    context.arc(center.x, center.y, 20.0, 0.0, 2.0 * std::f64::consts::PI);
    context.set_source_rgba(0.8, 0.8, 0.8, 0.5);
    context.fill();

    context.move_to(center.x - 10.0, center.y);
    context.line_to(center.x, center.y + 10.0);
    context.line_to(center.x + 12.0, center.y - 15.0);
    context.set_line_width(3.0);
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.stroke();
}

/// Draw an image either centered at original size if it fits within the target rectangle,
/// or shrunk to fit the target rectangle while keeping the aspect ratio.
pub fn draw_image_shrink_fit(context: &cairo::Context, surface: cairo::ImageSurface, target: &Rect) {
    let img_size = Size {
        w: surface.get_width() as f64,
        h: surface.get_height() as f64
    };
    if target.size.contains(&img_size) {
        let render_pos = target.centered(&img_size).top_left;
        context.set_source_surface(&*surface, render_pos.x, render_pos.y);
        context.paint()
    } else {
        context.save();
        let render_size = img_size.scale_to_fit(&target.size);
        let render_pos = target.centered(&render_size).top_left;
        let scale = render_size.w / img_size.w; // w or h doesn't matter, aspect ratio is kept

        context.translate(render_pos.x, render_pos.y);
        context.scale(scale, scale);
        context.set_source_surface(&*surface, 0.0, 0.0);
        context.paint();
        context.restore();
    }
}