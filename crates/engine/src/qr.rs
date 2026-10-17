//! Pure-Rust QR Code generator and vector/raster PDF embedder (ISO/IEC 18004).
//!
//! Generates QR codes with Reed-Solomon error correction and exports them as
//! razor-sharp PDF vector paths or high-resolution PNG images.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum QrError {
    #[error("text payload is too large for supported QR versions (max 134 bytes)")]
    PayloadTooLarge,
    #[error("failed to encode PNG: {0}")]
    PngEncoding(String),
}

/// A 2D boolean grid representing dark and light QR code modules.
#[derive(Clone, Debug, PartialEq)]
pub struct QrMatrix {
    /// Dimension in modules (e.g. 21 for v1, 25 for v2, etc.).
    pub size: usize,
    /// Row-major module data: true = dark/black, false = light/white.
    pub modules: Vec<bool>,
}

impl QrMatrix {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            modules: vec![false; size.saturating_mul(size)],
        }
    }

    pub fn is_dark(&self, row: usize, col: usize) -> bool {
        if row < self.size && col < self.size {
            self.modules.get(row.saturating_mul(self.size).saturating_add(col)).copied().unwrap_or(false)
        } else {
            false
        }
    }

    pub fn set(&mut self, row: usize, col: usize, dark: bool) {
        if row < self.size && col < self.size {
            if let Some(m) = self.modules.get_mut(row.saturating_mul(self.size).saturating_add(col)) {
                *m = dark;
            }
        }
    }

    /// Render QR matrix to PNG image bytes.
    pub fn to_png_bytes(&self, module_px: u32, border_modules: u32) -> Result<Vec<u8>, QrError> {
        let module_px = module_px.max(1);
        let border_modules = border_modules.max(1);
        let total_modules = (self.size as u32).saturating_add(border_modules.saturating_mul(2));
        let img_side = total_modules.saturating_mul(module_px);

        let mut rgba = vec![255u8; (img_side as usize).saturating_mul(img_side as usize).saturating_mul(4)];

        for r in 0..self.size {
            for c in 0..self.size {
                if self.is_dark(r, c) {
                    let start_x = ((c as u32).saturating_add(border_modules)).saturating_mul(module_px);
                    let start_y = ((r as u32).saturating_add(border_modules)).saturating_mul(module_px);

                    for py in start_y..start_y.saturating_add(module_px) {
                        for px in start_x..start_x.saturating_add(module_px) {
                            if px < img_side && py < img_side {
                                let idx = ((py as usize).saturating_mul(img_side as usize).saturating_add(px as usize)).saturating_mul(4);
                                if idx.saturating_add(3) < rgba.len() {
                                    rgba[idx] = 0;
                                    rgba[idx + 1] = 0;
                                    rgba[idx + 2] = 0;
                                    rgba[idx + 3] = 255;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, img_side, img_side);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| QrError::PngEncoding(e.to_string()))?;
            writer.write_image_data(&rgba).map_err(|e| QrError::PngEncoding(e.to_string()))?;
        }
        Ok(out)
    }

    /// Generate PDF vector path stream operators (`re` ... `f`) within `rect: [x, y, x + w, y + h]`.
    pub fn to_pdf_vector_ops(&self, rect: [f64; 4], border_modules: u32) -> String {
        let total_modules = (self.size as f64) + (border_modules as f64) * 2.0;
        let w = rect[2] - rect[0];
        let h = rect[3] - rect[1];
        let mod_w = w / total_modules.max(1.0);
        let mod_h = h / total_modules.max(1.0);

        let mut ops = String::with_capacity(self.size.saturating_mul(self.size).saturating_mul(24));
        ops.push_str("0 0 0 rg\n"); // Black fill

        for r in 0..self.size {
            for c in 0..self.size {
                if self.is_dark(r, c) {
                    let mx = rect[0] + ((c as f64) + (border_modules as f64)) * mod_w;
                    // PDF user space coordinates have y pointing upwards:
                    let my = rect[1] + ((self.size - 1 - r) as f64 + (border_modules as f64)) * mod_h;
                    use std::fmt::Write;
                    let _ = write!(ops, "{:.2} {:.2} {:.2} {:.2} re\n", mx, my, mod_w, mod_h);
                }
            }
        }
        ops.push_str("f\n");
        ops
    }
}

// Galois Field GF(256) with primitive polynomial 0x11D for Reed-Solomon ECC
const GF_POLY: u16 = 0x11D;
const GF_EXP: [u8; 512] = {
    let mut exp = [0u8; 512];
    let mut x = 1u16;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        exp[i + 255] = x as u8;
        x <<= 1;
        if (x & 0x100) != 0 {
            x ^= GF_POLY;
        }
        i += 1;
    }
    exp[510] = exp[0];
    exp[511] = exp[1];
    exp
};

const GF_LOG: [u8; 256] = {
    let mut log = [0u8; 256];
    let mut i = 0;
    while i < 255 {
        log[GF_EXP[i] as usize] = i as u8;
        i += 1;
    }
    log
};

fn gf_mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        0
    } else {
        GF_EXP[(GF_LOG[a as usize] as usize).saturating_add(GF_LOG[b as usize] as usize)]
    }
}

fn rs_compute_ecc(data: &[u8], num_ecc: usize) -> Vec<u8> {
    let mut generator = vec![1u8];
    for i in 0..num_ecc {
        let mut next = vec![0u8; generator.len() + 1];
        let factor = GF_EXP[i];
        for (j, &coef) in generator.iter().enumerate() {
            next[j] ^= coef;
            next[j + 1] ^= gf_mul(coef, factor);
        }
        generator = next;
    }

    let mut remainder = vec![0u8; num_ecc];
    for &byte in data {
        let lead = byte ^ remainder.get(0).copied().unwrap_or(0);
        for j in 0..num_ecc.saturating_sub(1) {
            let next = remainder.get(j + 1).copied().unwrap_or(0);
            let g = generator.get(j + 1).copied().unwrap_or(0);
            if let Some(r) = remainder.get_mut(j) {
                *r = next ^ gf_mul(lead, g);
            }
        }
        if let Some(r) = remainder.get_mut(num_ecc.saturating_sub(1)) {
            let g = generator.get(num_ecc).copied().unwrap_or(0);
            *r = gf_mul(lead, g);
        }
    }
    remainder
}

/// Version specs for Medium (M) error correction.
struct VersionSpec {
    version: usize,
    size: usize,
    total_data_bytes: usize,
    ecc_bytes: usize,
    align_pos: &'static [usize],
}

static VERSION_SPECS: &[VersionSpec] = &[
    VersionSpec { version: 1, size: 21, total_data_bytes: 16, ecc_bytes: 10, align_pos: &[] },
    VersionSpec { version: 2, size: 25, total_data_bytes: 28, ecc_bytes: 16, align_pos: &[6, 18] },
    VersionSpec { version: 3, size: 29, total_data_bytes: 44, ecc_bytes: 26, align_pos: &[6, 22] },
    VersionSpec { version: 4, size: 33, total_data_bytes: 64, ecc_bytes: 36, align_pos: &[6, 26] },
    VersionSpec { version: 5, size: 37, total_data_bytes: 86, ecc_bytes: 48, align_pos: &[6, 30] },
    VersionSpec { version: 6, size: 41, total_data_bytes: 108, ecc_bytes: 64, align_pos: &[6, 34] },
];

pub struct QrCode;

impl QrCode {
    /// Encode a text or URL payload into a `QrMatrix` using Medium error correction.
    pub fn encode_text(text: &str) -> Result<QrMatrix, QrError> {
        let bytes = text.as_bytes();
        let payload_len = bytes.len();

        // Find smallest version that fits payload in Byte mode:
        // Overhead: 4 bits mode + 8 bits char count = 12 bits = 1.5 bytes.
        let required_bytes = payload_len.saturating_add(2);
        let spec = VERSION_SPECS
            .iter()
            .find(|s| s.total_data_bytes >= required_bytes)
            .ok_or(QrError::PayloadTooLarge)?;

        // Build bitstream
        let mut bit_buf = Vec::new();
        // 1. Byte mode indicator: 0100
        push_bits(&mut bit_buf, 0b0100, 4);
        // 2. Character count indicator: 8 bits for versions 1-9
        push_bits(&mut bit_buf, payload_len as u32, 8);
        // 3. Payload bytes
        for &b in bytes {
            push_bits(&mut bit_buf, b as u32, 8);
        }
        // 4. Terminator (up to 4 zeroes)
        let max_data_bits = spec.total_data_bytes.saturating_mul(8);
        let term_len = 4.min(max_data_bits.saturating_sub(bit_buf.len()));
        push_bits(&mut bit_buf, 0, term_len);
        // 5. Pad to multiple of 8
        let rem = bit_buf.len() % 8;
        if rem != 0 {
            push_bits(&mut bit_buf, 0, 8 - rem);
        }
        // 6. Pad bytes 0xEC and 0x11
        let mut data_bytes = bits_to_bytes(&bit_buf);
        let mut pad = 0xECu8;
        while data_bytes.len() < spec.total_data_bytes {
            data_bytes.push(pad);
            pad = if pad == 0xEC { 0x11 } else { 0xEC };
        }

        // 7. Compute Reed-Solomon ECC
        let ecc_bytes = rs_compute_ecc(&data_bytes, spec.ecc_bytes);
        let mut codeword_bytes = data_bytes;
        codeword_bytes.extend_from_slice(&ecc_bytes);

        // 8. Place patterns and modules
        let mut matrix = QrMatrix::new(spec.size);
        let mut is_function = vec![false; spec.size.saturating_mul(spec.size)];

        // Finder patterns (top-left, top-right, bottom-left)
        place_finder_pattern(&mut matrix, &mut is_function, 0, 0);
        place_finder_pattern(&mut matrix, &mut is_function, 0, spec.size - 7);
        place_finder_pattern(&mut matrix, &mut is_function, spec.size - 7, 0);

        // Alignment patterns for version >= 2
        for &r in spec.align_pos {
            for &c in spec.align_pos {
                if !((r <= 8 && c <= 8) || (r <= 8 && c >= spec.size - 8) || (r >= spec.size - 8 && c <= 8)) {
                    place_alignment_pattern(&mut matrix, &mut is_function, r - 2, c - 2);
                }
            }
        }

        // Timing patterns
        for i in 8..spec.size - 8 {
            let dark = i % 2 == 0;
            set_function(&mut matrix, &mut is_function, 6, i, dark);
            set_function(&mut matrix, &mut is_function, i, 6, dark);
        }

        // Dark module
        set_function(&mut matrix, &mut is_function, (4 * spec.version).saturating_add(9), 8, true);

        // Reserve format info areas
        reserve_format_info(&mut is_function, spec.size);

        // Data placement in 2-module wide zigzag columns
        let mut bit_idx = 0;
        let total_bits = codeword_bytes.len().saturating_mul(8);
        let mut right = spec.size as i32 - 1;
        let mut upwards = true;

        while right > 0 {
            if right == 6 {
                right -= 1; // Skip vertical timing column
            }
            for row_offset in 0..spec.size {
                let r = if upwards { spec.size - 1 - row_offset } else { row_offset };
                for col_offset in 0..2 {
                    let c = (right - col_offset) as usize;
                    let idx = r * spec.size + c;
                    if !is_function.get(idx).copied().unwrap_or(true) {
                        let bit = if bit_idx < total_bits {
                            let byte = codeword_bytes.get(bit_idx / 8).copied().unwrap_or(0);
                            let shift = 7 - (bit_idx % 8);
                            (byte >> shift) & 1 == 1
                        } else {
                            false
                        };
                        bit_idx += 1;
                        // Apply Mask 0: (row + col) % 2 == 0
                        let masked_bit = bit ^ ((r + c) % 2 == 0);
                        matrix.set(r, c, masked_bit);
                    }
                }
            }
            right -= 2;
            upwards = !upwards;
        }

        // Write Format Information (ECC level M = 00, Mask 0 = 000 -> 0b00000 -> with BCH = 0x5412)
        write_format_info(&mut matrix, spec.size, 0b101010000010010 ^ 0b101010000010010);

        Ok(matrix)
    }
}

fn push_bits(buf: &mut Vec<bool>, val: u32, count: usize) {
    for i in (0..count).rev() {
        buf.push(((val >> i) & 1) == 1);
    }
}

fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((bits.len() + 7) / 8);
    for chunk in bits.chunks(8) {
        let mut b = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                b |= 1 << (7 - i);
            }
        }
        bytes.push(b);
    }
    bytes
}

fn set_function(matrix: &mut QrMatrix, is_func: &mut [bool], r: usize, c: usize, dark: bool) {
    matrix.set(r, c, dark);
    let idx = r * matrix.size + c;
    if let Some(f) = is_func.get_mut(idx) {
        *f = true;
    }
}

fn place_finder_pattern(matrix: &mut QrMatrix, is_func: &mut [bool], start_r: usize, start_c: usize) {
    for r in 0..7 {
        for c in 0..7 {
            let dark = r == 0 || r == 6 || c == 0 || c == 6 || (r >= 2 && r <= 4 && c >= 2 && c <= 4);
            set_function(matrix, is_func, start_r + r, start_c + c, dark);
        }
    }
    // White separators around finder
    let size = matrix.size;
    for i in 0..8 {
        if start_r + 7 < size && start_c + i < size {
            set_function(matrix, is_func, start_r + 7, start_c + i, false);
        }
        if start_r + i < size && start_c + 7 < size {
            set_function(matrix, is_func, start_r + i, start_c + 7, false);
        }
        if start_r >= 1 && start_c + i < size {
            set_function(matrix, is_func, start_r - 1, start_c + i, false);
        }
        if start_r + i < size && start_c >= 1 {
            set_function(matrix, is_func, start_r + i, start_c - 1, false);
        }
    }
}

fn place_alignment_pattern(matrix: &mut QrMatrix, is_func: &mut [bool], start_r: usize, start_c: usize) {
    for r in 0..5 {
        for c in 0..5 {
            let dark = r == 0 || r == 4 || c == 0 || c == 4 || (r == 2 && c == 2);
            set_function(matrix, is_func, start_r + r, start_c + c, dark);
        }
    }
}

fn reserve_format_info(is_func: &mut [bool], size: usize) {
    for i in 0..9 {
        if let Some(f) = is_func.get_mut(8 * size + i) { *f = true; }
        if let Some(f) = is_func.get_mut(i * size + 8) { *f = true; }
    }
    for i in (size.saturating_sub(8))..size {
        if let Some(f) = is_func.get_mut(8 * size + i) { *f = true; }
        if let Some(f) = is_func.get_mut(i * size + 8) { *f = true; }
    }
}

fn write_format_info(matrix: &mut QrMatrix, size: usize, bits: u16) {
    // 15 format bits (masked with 0x5412)
    let format = bits ^ 0x5412;
    for i in 0..15 {
        let bit = ((format >> i) & 1) == 1;
        // Around top-left finder
        if i <= 5 {
            matrix.set(8, i, bit);
        } else if i == 6 {
            matrix.set(8, 7, bit);
        } else if i == 7 {
            matrix.set(8, 8, bit);
        } else if i == 8 {
            matrix.set(7, 8, bit);
        } else {
            matrix.set(14 - i, 8, bit);
        }

        // Around right & bottom finders
        if i < 7 {
            matrix.set(size - 1 - i, 8, bit);
        } else {
            matrix.set(8, size - 15 + i, bit);
        }
    }
}
