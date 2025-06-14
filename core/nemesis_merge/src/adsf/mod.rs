pub mod path_parser;

use self::path_parser::{parse_adsf_path, ParsedAdsfPatchPath, ParserType};
use rayon::prelude::*;
use skyrim_anim_parser::adsf::patch::{parse_clip_anim_block_patch, parse_clip_motion_block_patch};
use skyrim_anim_parser::adsf::ser::serialize_alt_adsf;
use skyrim_anim_parser::adsf::{AltAdsf, ClipAnimDataBlock, ClipMotionBlock};
use snafu::ResultExt as _;

use std::path::{Path, PathBuf};

use crate::errors::{
    Error, FailedIoSnafu, FailedParseAdsfPatchSnafu, FailedParseAdsfTemplateSnafu,
};
use crate::results::partition_results;
use crate::types::{OwnedAdsfPatchMap, PriorityMap};
use crate::Config;

#[derive(Debug, PartialEq, Default)]
pub struct AdsfPatch<'a> {
    /// e.g. `DefaultMale`, `DefaultFemale`
    pub target: &'a str,
    /// e.g. `dmco`, `slide`
    pub id: &'a str,
    pub patch: PatchKind<'a>,
}

#[derive(Debug, PartialEq)]
pub enum PatchKind<'a> {
    Anim(ClipAnimDataBlock<'a>),
    Motion(ClipMotionBlock<'a>),
}

impl<'a> Default for PatchKind<'a> {
    #[inline]
    fn default() -> Self {
        Self::Anim(ClipAnimDataBlock::default())
    }
}

const ADSF_INNER_PATH: &str = "meshes/animationdatasinglefile.bin";

// "dmco", "slide"
/// Patch to `animationdatasinglefile.txt`
pub(crate) fn apply_adsf_patches(
    owned_anim_data_patches: OwnedAdsfPatchMap,
    id_order: &PriorityMap,
    config: &Config,
) -> Vec<Error> {
    // 1/5 Parse adsf patch (1 loop with par_iter)
    let results: Vec<Result<AdsfPatch, Error>> = owned_anim_data_patches
        .0
        .par_iter() // par iter
        .map(parse_anim_data_patch)
        .collect(); // back iter

    let (mut borrowed_patches, mut errors) = partition_results(results);

    // 2/5 Sort by priority ids.(to vec 2 loop) => borrowed_map
    sort_patches_by_priority(&mut borrowed_patches, id_order);

    macro_rules! bail {
        ($expr:expr) => {
            match $expr {
                Ok(adsf) => adsf,
                Err(err) => {
                    errors.push(err);
                    return errors;
                }
            }
        };
    }

    // 3/5 read template adsf.
    let alt_adsf_bytes = bail!(read_adsf_file(config));
    let mut alt_adsf: AltAdsf = bail!(rmp_serde::from_slice(&alt_adsf_bytes).with_context(|_| {
        FailedParseAdsfTemplateSnafu {
            path: config.resource_dir.join(ADSF_INNER_PATH),
        }
    }));

    // 4/5. Apply adsf patch to base adsf(anim_data & motion data).
    for adsf_patch in borrowed_patches {
        if let Some(anim_data) = alt_adsf.0.get_mut(adsf_patch.target) {
            match adsf_patch.patch {
                PatchKind::Anim(clip_anim_data_block) => {
                    anim_data.add_clip_anim_blocks.push(clip_anim_data_block);
                }
                PatchKind::Motion(clip_motion_block) => {
                    anim_data.add_clip_motion_blocks.push(clip_motion_block);
                }
            };
        }
    }

    // 5/5 Write adsf.
    let mut output_path = config.output_dir.join(ADSF_INNER_PATH);
    output_path.set_extension("txt");
    bail!(write_alt_adsf_file(output_path, &alt_adsf));

    errors
}

fn parse_anim_data_patch<'a>(
    (path, (adsf_patch, _priority)): (&'a PathBuf, &'a (String, usize)),
) -> Result<AdsfPatch<'a>, Error> {
    let ParsedAdsfPatchPath {
        target, // e.g. DefaultFemale
        id,     // e.g. slide
        parser_type,
        op: _,
    } = parse_adsf_path(path)?;

    let patch = match parser_type {
        ParserType::Anim => PatchKind::Anim(
            parse_clip_anim_block_patch(adsf_patch)
                .with_context(|_| FailedParseAdsfPatchSnafu { path: path.clone() })?,
        ),
        ParserType::Motion => PatchKind::Motion(
            parse_clip_motion_block_patch(adsf_patch)
                .with_context(|_| FailedParseAdsfPatchSnafu { path: path.clone() })?,
        ),
        ParserType::AnimHeader => {
            return Err(Error::Custom {
                msg: "Unsupported $header$ yet.".to_owned(),
            })
        }
    };
    Ok(AdsfPatch { target, id, patch })
}

/// Sorts AdsfPatch list based on the given ID priority list.
fn sort_patches_by_priority(patches: &mut [AdsfPatch], id_order: &PriorityMap) {
    patches.par_sort_by_key(|patch| id_order.get(patch.id).copied().unwrap_or(usize::MAX));
}

/// Read the ADSF file from the resource directory
fn read_adsf_file(config: &Config) -> Result<Vec<u8>, Error> {
    let adsf_read_path = config.resource_dir.join(ADSF_INNER_PATH);
    let adsf_string = std::fs::read(&adsf_read_path).with_context(|_| FailedIoSnafu {
        path: adsf_read_path,
    })?;
    Ok(adsf_string)
}

/// Write a single adsf file
fn write_alt_adsf_file(path: impl AsRef<Path>, alt_adsf: &AltAdsf) -> Result<(), Error> {
    let serialized = serialize_alt_adsf(alt_adsf);
    let path = path.as_ref();
    if let Some(parent_dir) = path.parent() {
        let _ = std::fs::create_dir_all(parent_dir);
    }
    std::fs::write(path, serialized).with_context(|_| FailedIoSnafu {
        path: path.to_path_buf(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_patches_by_priority() {
        let ids = ["dmco", "flinch", "a", "slide"];

        let mut patches = vec![
            AdsfPatch {
                id: ids[1],
                ..Default::default()
            }, // flinch
            AdsfPatch {
                id: ids[2],
                ..Default::default()
            }, // a
            AdsfPatch {
                id: ids[0],
                ..Default::default()
            }, // dmco
            AdsfPatch {
                id: ids[3],
                ..Default::default()
            },
        ];

        sort_patches_by_priority(
            &mut patches,
            &ids.iter().enumerate().map(|(i, &p)| (p, i)).collect(),
        );

        let sorted_ids: Vec<&str> = patches.iter().map(|p| p.id).collect();
        assert_eq!(sorted_ids, ids);
    }
}
