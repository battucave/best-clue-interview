// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod activate;
mod api;
mod shortcuts;
mod window;
mod db;
mod capture;
use tauri_plugin_http;
#[cfg(target_os = "macos")]
use tauri_plugin_macos_permissions;
use tauri_plugin_posthog::{init as posthog_init, PostHogConfig, PostHogOptions};
use tauri::{Manager, AppHandle, WebviewWindow};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
mod speaker;
use speaker::VadConfig;
use capture::CaptureState;

#[cfg(target_os = "macos")]
#[allow(deprecated)]
use tauri_nspanel::{
    cocoa::appkit::NSWindowCollectionBehavior, panel_delegate, WebviewWindowExt,
  };

  #[derive(Default)]
pub struct AudioState {
    stream_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    vad_config: Arc<Mutex<VadConfig>>,
    is_capturing: Arc<Mutex<bool>>,
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Get PostHog API key
    let posthog_api_key = option_env!("POSTHOG_API_KEY")
        .unwrap_or("")
        .to_string();
    let mut builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:pluely.db", db::migrations())
                .build(),
        )
        .manage(AudioState::default())
        .manage(CaptureState::default())
        .manage(shortcuts::WindowVisibility {
            is_hidden: Mutex::new(false),
        })
        .manage(shortcuts::RegisteredShortcuts::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_keychain::init())
        .plugin(tauri_plugin_shell::init()) // Add shell plugin
        .plugin(posthog_init(PostHogConfig {
            api_key: posthog_api_key,
            options: Some(PostHogOptions {
                // disable session recording
                disable_session_recording: Some(true),
                // disable pageview
                capture_pageview: Some(false),
                // disable pageleave
                capture_pageleave: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        }))
        .plugin(tauri_plugin_machine_uid::init());
        #[cfg(target_os = "macos")]
        {
            builder = builder.plugin(tauri_nspanel::init());
        }
        let mut builder = builder.invoke_handler(tauri::generate_handler![
            get_app_version,
            window::set_window_height,
            capture::capture_to_base64,
            capture::start_screen_capture,
            capture::capture_selected_area,
            capture::close_overlay_window,
            shortcuts::check_shortcuts_registered,
            shortcuts::get_registered_shortcuts,
            shortcuts::update_shortcuts,
            shortcuts::validate_shortcut_key,
            shortcuts::set_app_icon_visibility,
            shortcuts::set_always_on_top,
            activate::activate_license_api,
            activate::deactivate_license_api,
            activate::validate_license_api,
            activate::mask_license_key_cmd,
            activate::get_checkout_url,
            activate::secure_storage_save,
            activate::secure_storage_get,
            activate::secure_storage_remove,
            api::transcribe_audio,
            api::chat_stream,
            api::fetch_models,
            api::create_system_prompt,
            api::check_license_status,
            speaker::start_system_audio_capture,
            speaker::stop_system_audio_capture,
            speaker::manual_stop_continuous,
            speaker::check_system_audio_access,
            speaker::request_system_audio_access,
            speaker::get_vad_config,
            speaker::update_vad_config,
            speaker::get_capture_status,
            speaker::get_audio_sample_rate,
            speaker::list_system_audio_devices,
            speaker::get_default_audio_device,
        ])
        .setup(|app| {
            // Setup main window positioning
            window::setup_main_window(app).expect("Failed to setup main window");
            #[cfg(target_os = "macos")]
            init(app.app_handle());

            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::MacosLauncher;

                #[allow(deprecated, unexpected_cfgs)]
                if let Err(e) = app.handle().plugin(tauri_plugin_autostart::init(
                    MacosLauncher::LaunchAgent,
                    Some(vec![]),
                )) {
                    eprintln!("Failed to initialize autostart plugin: {}", e);
                }
            }

            // Initialize global shortcut plugin with centralized handler
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};
                        
                        if event.state() == ShortcutState::Pressed {
                            // Get registered shortcuts and find matching action
                            let state = app.state::<shortcuts::RegisteredShortcuts>();
                            let registered = match state.shortcuts.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => {
                                    eprintln!("Mutex poisoned in handler, recovering...");
                                    poisoned.into_inner()
                                }
                            };
                            
                            // Find which action this shortcut maps to
                            for (action_id, shortcut_str) in registered.iter() {
                                if let Ok(s) = shortcut_str.parse::<Shortcut>() {
                                    if &s == shortcut {
                                        eprintln!("Shortcut triggered: {} ({})", action_id, shortcut_str);
                                        shortcuts::handle_shortcut_action(&app, action_id);
                                        break;
                                    }
                                }
                            }
                        }
                    })
                    .build(),
            ).expect("Failed to initialize global shortcut plugin");
            if let Err(e) = shortcuts::setup_global_shortcuts(app.handle()) {
                eprintln!("Failed to setup global shortcuts: {}", e);
            }
           Ok(())
        });

    // Add macOS-specific permissions plugin
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_plugin_macos_permissions::init());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
fn init(app_handle: &AppHandle) {
    let window: WebviewWindow = app_handle.get_webview_window("main").unwrap();
  
    let panel = window.to_panel().unwrap();
  
    let delegate = panel_delegate!(MyPanelDelegate {
      window_did_become_key,
      window_did_resign_key
    });
  
    let handle = app_handle.to_owned();
  
    delegate.set_listener(Box::new(move |delegate_name: String| {
      match delegate_name.as_str() {
        "window_did_become_key" => {
          let app_name = handle.package_info().name.to_owned();
  
          println!("[info]: {:?} panel becomes key window!", app_name);
        }
        "window_did_resign_key" => {
          println!("[info]: panel resigned from key window!");
        }
        _ => (),
      }
    }));
  
    // Set the window to float level
    #[allow(non_upper_case_globals)]
    const NSFloatWindowLevel: i32 = 4;
    panel.set_level(NSFloatWindowLevel);
  
    #[allow(non_upper_case_globals)]
    const NSWindowStyleMaskNonActivatingPanel: i32 = 1 << 7;
    panel.set_style_mask(NSWindowStyleMaskNonActivatingPanel);
  
    #[allow(deprecated)]
    panel.set_collection_behaviour(
      NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces,
    );
  
    panel.set_delegate(delegate);
  }
