//! SocialEye — local-first social media & content management.
pub mod app;
pub mod automations;
pub mod commands;
pub mod connectors;
pub mod csv_import;
pub mod db;
pub mod error;
pub mod http;
pub mod ig_export;
pub mod oauth;
pub mod reports;
pub mod secrets;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = db::Db::open(&dir.join("socialeye.db")).map_err(|e| e.to_string())?;
            let state = app::AppState::new(db, Box::new(secrets::KeyringStore), dir).map_err(|e| e.to_string())?;
            // Re-allow previously imported media for preview.
            for m in db::query_json(&state.db.conn(), "SELECT path FROM media_assets", []).unwrap_or_default() {
                if let Some(p) = m["path"].as_str() {
                    let _ = app.asset_protocol_scope().allow_file(p);
                }
            }
            app.manage(state);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let ui = commands::TauriUi(handle.clone());
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                loop {
                    handle.state::<app::AppState>().tick(&ui).await;
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::providers_list,
            commands::provider_save,
            commands::capabilities,
            commands::accounts_list,
            commands::account_connect,
            commands::account_disconnect,
            commands::account_remove,
            commands::account_set_auto_sync,
            commands::account_sync,
            commands::sync_runs,
            commands::spotify_add_podcast,
            commands::spotify_saved_shows,
            commands::content_list,
            commands::metrics_series,
            commands::metrics_latest,
            commands::breakdowns,
            commands::youtube_retention,
            commands::youtube_update_video,
            commands::comments_list,
            commands::comment_moderate,
            commands::media_add,
            commands::media_list,
            commands::media_set_public_url,
            commands::media_remove,
            commands::draft_save,
            commands::drafts_list,
            commands::draft_delete,
            commands::variants_list,
            commands::variant_save,
            commands::variant_delete,
            commands::variant_validate,
            commands::variant_publish,
            commands::variant_unschedule,
            commands::publish_history,
            commands::templates_list,
            commands::template_save,
            commands::template_delete,
            commands::calendar_items,
            commands::automations_list,
            commands::automation_save,
            commands::automation_set_enabled,
            commands::automation_delete,
            commands::automation_runs,
            commands::notifications_list,
            commands::notifications_mark_read,
            commands::notifications_clear,
            commands::ig_import,
            commands::ig_imports_list,
            commands::ig_import_delete,
            commands::ig_compare,
            commands::csv_import_metrics,
            commands::export_report,
            commands::backup_create,
            commands::backup_restore,
            commands::data_export_json,
            commands::search,
            commands::audit_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SocialEye");
}
