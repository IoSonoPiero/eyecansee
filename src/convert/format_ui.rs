//! The format of Save as… and its options: quality, colour detail, compression, metadata. The
//! options are also those of Convert… (`batch_page`).

use eframe::egui;

use super::{Window, keeps_hdr};
use crate::batch::with_ext;
use crate::encode::{Chroma, Format, Options, Original, PngFilter, TiffCompression};
use crate::i18n::I18n;

/// The output format; the name takes its extension.
pub(super) fn format_choice(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    let o = &mut w.options;
    let before = o.format;
    egui::ComboBox::from_label(i18n.t("convert-format")).selected_text(o.format.label()).show_ui(ui, |ui| {
        for f in Format::ALL {
            ui.selectable_value(&mut o.format, f, f.label());
        }
    });
    if o.format != before {
        w.name = with_ext(&w.name, o.format);
    }
}

fn quality(ui: &mut egui::Ui, v: &mut u8, label: String) {
    ui.add(egui::Slider::new(v, 1..=100).text(label));
}

/// The options of the chosen format's encoder, for an image with transparency (`alpha`) or larger
/// than an icon (`big`); with several images, as if they all were.
pub(crate) fn format_options(ui: &mut egui::Ui, o: &mut Options, alpha: bool, big: bool, i18n: &I18n) {
    let t = |id| i18n.t(id);
    if alpha && matches!(o.format, Format::Jpeg | Format::Hdr) {
        ui.weak(t("convert-on-white"));
    }
    match o.format {
        Format::Jpeg => jpeg_options(ui, o, i18n),
        Format::Png => png_options(ui, o, i18n),
        Format::Webp => webp_options(ui, o, i18n),
        Format::Avif => {
            quality(ui, &mut o.avif_quality, t("convert-quality"));
            if alpha {
                quality(ui, &mut o.avif_alpha_quality, t("convert-alpha-quality"));
            }
            ui.add(egui::Slider::new(&mut o.avif_speed, 1..=10).text(t("convert-speed"))).on_hover_text(t("convert-speed-hint"));
            ui.checkbox(&mut o.avif_ten_bit, t("convert-ten-bit"));
            ui.checkbox(&mut o.avif_rgb, t("convert-avif-rgb"));
        }
        Format::Tiff => tiff_options(ui, o, i18n),
        Format::Gif => {
            ui.add(egui::Slider::new(&mut o.gif_quality, 1..=30).text(t("convert-gif-quality")));
        }
        Format::Tga => {
            ui.checkbox(&mut o.tga_rle, t("convert-tga-rle"));
        }
        Format::Ico if big => {
            ui.weak(t("convert-ico-reduced"));
        }
        Format::Bmp | Format::Qoi | Format::Ico | Format::Pnm | Format::Exr | Format::Hdr | Format::Farbfeld => {}
    }
}

fn jpeg_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.add_enabled_ui(!o.jpeg_lossless, |ui| {
        quality(ui, &mut o.jpeg_quality, t("convert-quality"));
        egui::ComboBox::from_label(t("convert-chroma"))
            .selected_text(t(match o.chroma {
                Chroma::Full => "convert-chroma-full",
                Chroma::Half => "convert-chroma-half",
                Chroma::Quarter => "convert-chroma-quarter",
                Chroma::Grey => "convert-chroma-grey",
            }))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut o.chroma, Chroma::Full, t("convert-chroma-full"));
                ui.selectable_value(&mut o.chroma, Chroma::Half, t("convert-chroma-half"));
                ui.selectable_value(&mut o.chroma, Chroma::Quarter, t("convert-chroma-quarter"));
                ui.selectable_value(&mut o.chroma, Chroma::Grey, t("convert-chroma-grey"));
            });
    });
    ui.checkbox(&mut o.progressive, t("convert-progressive"));
    ui.checkbox(&mut o.optimize, t("convert-optimize"));
    ui.checkbox(&mut o.arithmetic, t("convert-arithmetic"));
    ui.checkbox(&mut o.jpeg_lossless, t("convert-jpeg-lossless"));
}

fn png_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.add(egui::Slider::new(&mut o.png_level, 0..=9).text(t("convert-png-level")));
    let filters = [
        (PngFilter::Adaptive, t("convert-filter-adaptive")),
        (PngFilter::None, t("convert-none")),
        (PngFilter::Sub, "Sub".to_string()),
        (PngFilter::Up, "Up".to_string()),
        (PngFilter::Avg, "Average".to_string()),
        (PngFilter::Paeth, "Paeth".to_string()),
    ];
    let shown = filters.iter().find(|(f, _)| *f == o.png_filter).map(|(_, l)| l.clone()).unwrap_or_default();
    egui::ComboBox::from_label(t("convert-png-filter")).selected_text(shown).show_ui(ui, |ui| {
        for (f, label) in filters {
            ui.selectable_value(&mut o.png_filter, f, label);
        }
    });
}

fn webp_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.horizontal(|ui| {
        ui.radio_value(&mut o.webp_lossless, false, t("convert-lossy"));
        ui.radio_value(&mut o.webp_lossless, true, t("convert-lossless-mode"));
    });
    if o.webp_lossless {
        ui.add(egui::Slider::new(&mut o.near_lossless, 0..=100).text(t("convert-near-lossless")));
    } else {
        quality(ui, &mut o.webp_quality, t("convert-quality"));
        ui.checkbox(&mut o.sharp_yuv, t("convert-sharp-yuv"));
    }
    ui.add(egui::Slider::new(&mut o.webp_effort, 0..=6).text(t("convert-effort"))).on_hover_text(t("convert-speed-hint"));
}

fn tiff_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let kinds = [
        (TiffCompression::None, t("convert-none")),
        (TiffCompression::Lzw, "LZW".to_string()),
        (TiffCompression::DeflateFast, t("convert-deflate-fast")),
        (TiffCompression::Deflate, "Deflate".to_string()),
        (TiffCompression::DeflateBest, t("convert-deflate-best")),
        (TiffCompression::PackBits, "PackBits".to_string()),
    ];
    let shown = kinds.iter().find(|(k, _)| *k == o.tiff).map(|(_, l)| l.clone()).unwrap_or_default();
    egui::ComboBox::from_label(t("convert-compression")).selected_text(shown).show_ui(ui, |ui| {
        for (k, label) in kinds {
            ui.selectable_value(&mut o.tiff, k, label);
        }
    });
    let predictor = matches!(o.tiff, TiffCompression::Lzw | TiffCompression::DeflateFast | TiffCompression::Deflate | TiffCompression::DeflateBest);
    ui.add_enabled(predictor, egui::Checkbox::new(&mut o.tiff_predictor, t("convert-predictor")));
}

/// Whether the format loses quality, and the metadata kept when both the original and the format have some.
pub(super) fn meta_options(ui: &mut egui::Ui, o: &mut Options, original: &Original, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let lossless = match o.format {
        Format::Png | Format::Tiff | Format::Bmp | Format::Qoi | Format::Tga | Format::Pnm | Format::Exr | Format::Farbfeld => true,
        Format::Ico => original.image.width().max(original.image.height()) <= crate::encode::ICO_SIDE,
        Format::Webp => o.webp_lossless && o.near_lossless == 100,
        Format::Jpeg => o.jpeg_lossless,
        _ => false,
    };
    if lossless {
        ui.weak(t("convert-lossless"));
    } else if o.format == Format::Gif {
        ui.weak(t("convert-gif"));
    }
    let (can_meta, hint) = original.meta_box(o.format);
    let r = ui.add_enabled(can_meta, egui::Checkbox::new(&mut o.keep_meta, t(o.format.meta_label())));
    if let Some(hint) = hint {
        r.on_hover_text(t(hint)).on_disabled_hover_text(t(hint));
    }
    if original.hdr.is_some() && !keeps_hdr(o) {
        ui.label(crate::theme::alert(t("convert-ultra-hdr-lost")));
    }
}
