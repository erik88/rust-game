//! Loading PNG images into SDL textures without the SDL2_image extension.
//!
//! SDL2_image is only a decoder convenience: it reads a PNG into an
//! `SDL_Surface` and then calls core SDL2 to upload it to a texture. We do the
//! same here, decoding with the pure-Rust `png` crate so the build (and the
//! shipped binaries) depend only on core SDL2 — no external image-codec
//! libraries. The resulting `Texture` is identical to one from
//! `load_texture`, so rendering is unchanged.

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Texture, TextureCreator};
use sdl2::surface::Surface;

/// Decode the PNG at `path` and upload it to a GPU texture owned by `creator`.
pub fn load_png_texture<'a, T>(
    creator: &'a TextureCreator<T>,
    path: &str,
) -> Result<Texture<'a>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(|e| format!("{path}: {e}"))?;

    let mut data = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut data)
        .map_err(|e| format!("{path}: {e}"))?;

    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{path}: unsupported PNG bit depth {:?}", info.bit_depth));
    }

    // Normalise to 32-bit RGBA so we can use a single SDL pixel format. The png
    // crate yields bytes in R,G,B,A memory order; on little-endian targets
    // (x86-64, our only targets) that maps to SDL's ABGR8888.
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data,
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 0xff])
            .collect(),
        other => return Err(format!("{path}: unsupported PNG color type {other:?}")),
    };

    let pitch = info.width * 4;
    let mut rgba = rgba;
    let surface = Surface::from_data(
        &mut rgba,
        info.width,
        info.height,
        pitch,
        PixelFormatEnum::ABGR8888,
    )?;

    creator
        .create_texture_from_surface(&surface)
        .map_err(|e| e.to_string())
}
