use crate::config::FontAttributes;
use crate::font::locator::{FontDataHandle, FontLocator};
use failure::Fallible;
use font_loader::system_fonts;

pub struct FontLoaderFontLocator {}

impl FontLocator for FontLoaderFontLocator {
    fn load_fonts(&self, fonts_selection: &[FontAttributes]) -> Fallible<Vec<FontDataHandle>> {
        let mut fonts = Vec::new();
        for font_attr in fonts_selection {
            let mut font_props =
                system_fonts::FontPropertyBuilder::new().family(&font_attr.family).monospace();
            font_props = if *font_attr.bold.as_ref().unwrap_or(&false) {
                font_props.bold()
            } else {
                font_props
            };
            font_props = if *font_attr.italic.as_ref().unwrap_or(&false) {
                font_props.italic()
            } else {
                font_props
            };
            let font_props = font_props.build();

            if let Some((data, index)) = system_fonts::get(&font_props) {
                let handle = FontDataHandle::Memory { data, index: index as u32 };
                fonts.push(handle);
            }
        }
        Ok(fonts)
    }
}
