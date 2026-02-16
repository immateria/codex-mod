use super::super::*;
use super::centered_target_rect_for_image;
use super::render_image_placeholder;
use ratatui::widgets::Widget;
use ratatui_image::Image;
use ratatui_image::Resize;
use ratatui_image::picker::Picker;

impl ChatWidget<'_> {
    pub(in super::super) fn render_screenshot_highlevel(
        &self,
        path: &PathBuf,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if area.width == 0 || area.height == 0 {
            render_image_placeholder(path, area, buf, "Browser");
            return;
        }

        // First, cheaply read image dimensions without decoding the full image.
        let (img_w, img_h) = match image::image_dimensions(path) {
            Ok(dim) => dim,
            Err(_) => {
                render_image_placeholder(path, area, buf, "Browser");
                return;
            }
        };

        let (cell_w, cell_h) = self.measured_font_size();
        // picker (Retina 2x workaround preserved)
        let mut cached_picker = self.cached_picker.borrow_mut();
        if cached_picker.is_none() {
            // If we didn't get a picker from terminal query at startup, create one from font size.
            let p = Picker::from_fontsize((cell_w, cell_h));
            *cached_picker = Some(p);
        }
        let Some(picker) = cached_picker.as_ref() else {
            render_image_placeholder(path, area, buf, "Browser");
            return;
        };

        let Some(target) = centered_target_rect_for_image(area, cell_w, cell_h, img_w, img_h) else {
            render_image_placeholder(path, area, buf, "Browser");
            return;
        };

        // cache by (path, target)
        let needs_recreate = {
            let cached = self.cached_image_protocol.borrow();
            match cached.as_ref() {
                Some((cached_path, cached_rect, _)) => cached_path != path || *cached_rect != target,
                None => true,
            }
        };
        if needs_recreate {
            // Only decode when we actually need to (path/target changed)
            let dyn_img = match image::ImageReader::open(path) {
                Ok(r) => match r.decode() {
                    Ok(img) => img,
                    Err(_) => {
                        render_image_placeholder(path, area, buf, "Browser");
                        return;
                    }
                },
                Err(_) => {
                    render_image_placeholder(path, area, buf, "Browser");
                    return;
                }
            };
            match picker.new_protocol(dyn_img, target, Resize::Fit(Some(FilterType::Lanczos3))) {
                Ok(protocol) => *self.cached_image_protocol.borrow_mut() = Some((path.clone(), target, protocol)),
                Err(_) => {
                    render_image_placeholder(path, area, buf, "Browser");
                    return;
                }
            }
        }

        if let Some((_, rect, protocol)) = &*self.cached_image_protocol.borrow() {
            let image = Image::new(protocol);
            Widget::render(image, *rect, buf);
        } else {
            render_image_placeholder(path, area, buf, "Browser");
        }
    }
}
