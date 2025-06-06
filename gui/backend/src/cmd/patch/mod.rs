mod mod_info_loader;

pub(crate) use mod_info_loader::{
    __cmd__get_skyrim_data_dir, __cmd__load_mods_info, get_skyrim_data_dir, load_mods_info,
};

use crate::cmd::{bail, sender, time};
use crate::error::NotFoundResourceDirSnafu;
use nemesis_merge::{behavior_gen, Config, DebugOptions, HackOptions, OutPutTarget, Status};
use once_cell::sync::Lazy;
use snafu::ResultExt as _;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{async_runtime::JoinHandle, Manager, Window};

static PATCH_TASK: Lazy<Mutex<Option<JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GuiPatchOptions {
    hack_options: Option<HackOptions>,
    debug: DebugOptions,
    output_target: OutPutTarget,
    auto_remove_meshes: bool,
}

/// - ids: `e.g. vec!["../../dummy/Data/Nemesis_Engine/mod/aaaaa"]`
#[tauri::command]
pub(crate) async fn patch(
    window: Window,
    output: PathBuf,
    ids: Vec<PathBuf>,
    options: GuiPatchOptions,
) -> Result<(), String> {
    cancel_patch_inner()?; // Abort previous task if exists

    if options.auto_remove_meshes {
        let meshes_path = output.join("meshes");
        tauri::async_runtime::spawn(async move {
            let _ = tokio::fs::remove_dir_all(meshes_path).await;
        });
        let debug_path = output.join(".debug");
        tauri::async_runtime::spawn(async move {
            let _ = tokio::fs::remove_dir_all(debug_path).await;
        });
    }

    let handle = tauri::async_runtime::spawn({
        let resource_dir = {
            let resolver = window.app_handle().path();
            resolver
                .resource_dir()
                .context(NotFoundResourceDirSnafu)
                .or_else(|err| bail!(err))?
                .join("assets/templates/")
        };

        async move {
            let status_reporter = sender::<Status>(window, "d_merge://progress/patch");
            let config = Config {
                output_dir: output,
                resource_dir,
                status_report: Some(Box::new(status_reporter)),
                hack_options: options.hack_options,
                debug: options.debug,
                output_target: options.output_target,
            };

            let _ = time!("[patch]", behavior_gen(ids, config).await);
        }
    });

    match PATCH_TASK.lock() {
        Ok(mut guard) => {
            guard.replace(handle);
            Ok(())
        }
        Err(err) => bail!(format!("Failed to acquire lock: {err}")),
    }
}

#[tauri::command]
pub fn cancel_patch() -> Result<(), String> {
    cancel_patch_inner()
}

fn cancel_patch_inner() -> Result<(), String> {
    match PATCH_TASK.lock() {
        Ok(mut guard) => {
            if let Some(handle) = guard.take() {
                handle.abort();
            }

            Ok(())
        }
        Err(err) => {
            bail!(format!("Failed to acquire lock: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_option() {
        let gui_options = GuiPatchOptions {
            hack_options: Some(HackOptions::enable_all()),
            debug: DebugOptions::enable_all(),
            output_target: OutPutTarget::SkyrimSe,
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&gui_options).unwrap();
        println!("{json}");
    }
}
