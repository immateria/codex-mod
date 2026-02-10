// Use img_hash's re-exported image module for compatibility
use img_hash::image::{DynamicImage, io::Reader as ImageReader};
use img_hash::{HashAlg, HasherConfig, ImageHash};
use std::path::Path;

fn phash_256(img: &DynamicImage) -> ImageHash<[u8; 32]> {
    // "Classic" pHash ≈ Mean + DCT, larger hash for sensitivity
    HasherConfig::with_bytes_type::<[u8; 32]>()
        .hash_size(16, 16)
        .hash_alg(HashAlg::Mean)
        .preproc_dct()
        .to_hasher()
        .hash_image(img)
}

fn dhash_256(img: &DynamicImage) -> ImageHash<[u8; 32]> {
    // Gradient (dHash); good at catching small edge changes
    HasherConfig::with_bytes_type::<[u8; 32]>()
        .hash_size(16, 16)
        .hash_alg(HashAlg::Gradient)
        .to_hasher()
        .hash_image(img)
}

/// Compute a hash for an image that can be stored and compared later
pub fn compute_image_hash<P: AsRef<Path>>(path: P) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let img = ImageReader::open(path)?.decode()?;
    let phash = phash_256(&img);
    let dhash = dhash_256(&img);

    Ok((phash.as_bytes().to_vec(), dhash.as_bytes().to_vec()))
}

/// Compare image hashes to determine if images are similar
pub fn are_hashes_similar(phash1: &[u8], dhash1: &[u8], phash2: &[u8], dhash2: &[u8]) -> bool {
    if phash1.len() != 32 || dhash1.len() != 32 || phash2.len() != 32 || dhash2.len() != 32 {
        return false;
    }

    // Count differing bits (Hamming distance)
    let phash_dist = phash1
        .iter()
        .zip(phash2.iter())
        .map(|(a, b)| (a ^ b).count_ones())
        .sum::<u32>();

    let dhash_dist = dhash1
        .iter()
        .zip(dhash2.iter())
        .map(|(a, b)| (a ^ b).count_ones())
        .sum::<u32>();

    // 256 bits → ~5% tolerance (≈13 bits)
    phash_dist <= 13 && dhash_dist <= 13
}
