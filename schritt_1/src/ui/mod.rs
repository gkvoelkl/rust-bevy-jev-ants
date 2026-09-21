pub mod hud;
pub mod queen_input;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::ants::components::LastDecision;
use crate::decisions::{ActiveSource, DecisionStats, QueenOrder};

use queen_input::{OrderDraft, OrderHistory};

/// Intent and confidence over every ant. On by default — it is the whole point
/// of the demo. F3 toggles it: a key that never lands in the text field.
#[derive(Resource)]
pub struct DebugLayer(pub bool);

impl Default for DebugLayer {
    fn default() -> Self {
        Self(true)
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<OrderDraft>()
            .init_resource::<OrderHistory>()
            .init_resource::<DebugLayer>()
            .add_systems(Update, (toggle_debug_layer, hud::tint_by_confidence))
            .add_systems(EguiPrimaryContextPass, draw);
    }
}

fn toggle_debug_layer(keys: Res<ButtonInput<KeyCode>>, mut layer: ResMut<DebugLayer>) {
    if keys.just_pressed(KeyCode::F3) {
        layer.0 = !layer.0;
    }
}

/// The three resources the input bar works on. They only make sense together:
/// what is typed, what was said, and what was said before.
#[derive(SystemParam)]
struct OrderState<'w> {
    draft: ResMut<'w, OrderDraft>,
    order: ResMut<'w, QueenOrder>,
    history: ResMut<'w, OrderHistory>,
}

/// One root `Ui` for the whole frame. egui 0.36 hangs panels off a `Ui`, and two
/// independent roots would each lay out as if the other were not there.
fn draw(
    mut contexts: EguiContexts,
    mut orders: OrderState,
    stats: Res<DecisionStats>,
    layer: Res<DebugLayer>,
    source: Res<ActiveSource>,
    ants: Query<(&Transform, &LastDecision)>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    hud::top_bar(&mut viewport, &stats, source.0.name(), layer.0);
    queen_input::bottom_bar(
        &mut viewport,
        &mut orders.draft,
        &mut orders.order,
        &mut orders.history,
    );

    if layer.0 {
        hud::intent_overlay(&mut viewport, ctx.viewport_rect().center(), &ants);
    }

    Ok(())
}
