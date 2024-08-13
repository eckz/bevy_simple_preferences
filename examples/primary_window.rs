//! Shows how to store the primary window position in preferences.

use bevy::a11y::Focus;
use bevy::app::App;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::*;
use bevy::DefaultPlugins;
use bevy_simple_preferences::*;

// This struct has the same fields as Window, but only the ones we want to store in the preferences.
// It saves the mode, the position and the size of the window, but not the monitor, which is way more complicated
// than what we can solve in this example.
#[derive(Reflect)]
struct PrimaryWindowPreferences {
    pub mode: WindowMode,
    pub position: WindowPosition,
    pub resolution: WindowResolution,
}

impl Default for PrimaryWindowPreferences {
    fn default() -> Self {
        Self::from_reflect(&Window::default()).unwrap()
    }
}

struct PrimaryWindowPreferencesPlugin;

impl Plugin for PrimaryWindowPreferencesPlugin {
    fn build(&self, app: &mut App) {
        app.register_preferences::<PrimaryWindowPreferences>()
            .register_type::<Option<WindowTheme>>()
            .add_systems(Startup, spawn_primary_window)
            .add_systems(PreUpdate, save_primary_window_preferences);
    }
}

fn spawn_primary_window(
    mut commands: Commands,
    window_preferences: Preferences<PrimaryWindowPreferences>,
    focus: Option<ResMut<Focus>>,
) {
    let window = Window::from_reflect(&*window_preferences).unwrap();
    let initial_focus = commands.spawn(window).insert(PrimaryWindow).id();

    if let Some(mut focus) = focus {
        **focus = Some(initial_focus);
    }
}

fn save_primary_window_preferences(
    primary_window: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>,
    mut window_preferences: Preferences<PrimaryWindowPreferences>,
) {
    if let Ok(window) = primary_window.get_single() {
        *window_preferences = PrimaryWindowPreferences::from_reflect(window).unwrap();
    }
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(LogPlugin {
                    filter: "wgpu=error,naga=warn,bevy_simple_preferences=debug".into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: None,
                    ..default()
                }),
        )
        .add_plugins(PreferencesPlugin::persisted_with_app_name(
            "PreferencesExamplePrimaryWindow",
        ))
        .add_plugins(PrimaryWindowPreferencesPlugin)
        .run();
}
