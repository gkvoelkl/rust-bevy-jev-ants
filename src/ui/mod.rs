pub mod hud;
pub mod inspector;
pub mod key_prompt;
pub mod lab;
pub mod queen_input;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::ants::components::{AntId, LastDecision, LastExchange};
use crate::config::CELL_SIZE;
use crate::decisions::questions::Questions;
use crate::decisions::{
    ActiveSource, ColonyAwake, DecisionStats, DiscardLog, KeyPrompt, QueenOrder,
};
use crate::level::{Choice, Chosen, Levels};
use crate::world::fruit::Fruit;
use crate::world::grid::Grid;
use crate::world::nest::Stores;
use crate::world::render::View;
use crate::world::scenario::{Scenario, Solved};

use inspector::Selected;
use key_prompt::KeyDraft;
use lab::QuestionDraft;
use queen_input::{OrderChange, OrderDraft, OrderHistory, Task};

/// Everything the debug layer and the inspector read off an ant. One query, so
/// the picture above an ant and the window about it cannot drift apart.
pub type Ants<'w, 's> = Query<
    'w,
    's,
    (
        &'static AntId,
        &'static Transform,
        &'static LastDecision,
        &'static LastExchange,
    ),
>;

/// Intent and confidence over every ant. On by default — it is the whole point
/// of the demo. F3 toggles it: a key that never lands in the text field.
#[derive(Resource)]
pub struct DebugLayer(pub bool);

impl Default for DebugLayer {
    fn default() -> Self {
        Self(true)
    }
}

/// Whether the bench is open. Closed at the start: the board comes first, and
/// the bench covers a good part of it. The hint in the top bar is what points
/// at F2.
#[derive(Resource, Default)]
pub struct LabOpen(pub bool);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // The field starts on whatever the asset says, not on an empty box. The
        // fallback keeps the UI standalone — it must not depend on which plugin
        // was added first.
        let draft = app
            .world()
            .get_resource::<Questions>()
            .map(QuestionDraft::starting_from)
            .unwrap_or_default();

        app.add_plugins(EguiPlugin::default())
            .insert_resource(draft)
            .init_resource::<OrderDraft>()
            .init_resource::<OrderHistory>()
            .init_resource::<KeyDraft>()
            .init_resource::<DebugLayer>()
            .init_resource::<LabOpen>()
            .init_resource::<Selected>()
            .init_resource::<inspector::Tab>()
            .add_systems(Update, (toggle_panels, hud::tint_ants))
            .add_systems(EguiPrimaryContextPass, draw);
    }
}

fn toggle_panels(
    keys: Res<ButtonInput<KeyCode>>,
    mut layer: ResMut<DebugLayer>,
    mut lab: ResMut<LabOpen>,
) {
    if keys.just_pressed(KeyCode::F3) {
        layer.0 = !layer.0;
    }
    if keys.just_pressed(KeyCode::F2) {
        lab.0 = !lab.0;
    }
}

/// The four resources the input bar works on. They only make sense together:
/// what is typed, what was said, what was said before — and whether the queen
/// has ever spoken, which is a different thing from having said nothing.
#[derive(SystemParam)]
struct OrderState<'w> {
    draft: ResMut<'w, OrderDraft>,
    order: ResMut<'w, QueenOrder>,
    awake: Res<'w, ColonyAwake>,
    history: ResMut<'w, OrderHistory>,
}

/// What the colony has and what is left to fetch, plus the ants themselves.
#[derive(SystemParam)]
struct ColonyState<'w, 's> {
    stores: Res<'w, Stores>,
    fruits: Query<'w, 's, &'static Fruit>,
    ants: Ants<'w, 's>,
    /// Absent on the open field, which has no task to finish.
    scenario: Option<Res<'w, Scenario>>,
    solved: Res<'w, Solved>,
    levels: Res<'w, Levels>,
    chosen: ResMut<'w, Chosen>,
    grid: Res<'w, Grid>,
    view: ResMut<'w, View>,
}

/// The bench: which ant is being looked at, the question text, and what was
/// thrown away. All three are about the model rather than about the colony.
#[derive(SystemParam)]
struct LabState<'w> {
    open: ResMut<'w, LabOpen>,
    selected: ResMut<'w, Selected>,
    tab: ResMut<'w, inspector::Tab>,
    draft: ResMut<'w, QuestionDraft>,
    questions: ResMut<'w, Questions>,
    log: Res<'w, DiscardLog>,
}

/// What the top bar reads, and the one switch that belongs to the board rather
/// than to a panel. Grouped because the bar is the only reader of all of them.
#[derive(SystemParam)]
struct Readout<'w> {
    stats: Res<'w, DecisionStats>,
    source: Res<'w, ActiveSource>,
    layer: Res<'w, DebugLayer>,
}

/// The key dialog: whether it is being asked for, and what is being typed into
/// it. The first belongs to the decision layer — it is the one that knows there
/// is no key — and the second only to the screen.
#[derive(SystemParam)]
struct KeyState<'w> {
    prompt: ResMut<'w, KeyPrompt>,
    draft: ResMut<'w, KeyDraft>,
}

/// One root `Ui` for the whole frame. egui 0.36 hangs panels off a `Ui`, and two
/// independent roots would each lay out as if the other were not there.
fn draw(
    mut contexts: EguiContexts,
    mut orders: OrderState,
    readout: Readout,
    mut colony: ColonyState,
    mut bench: LabState,
    mut key: KeyState,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    // Before everything else, because until it is answered everything else is
    // scenery: the queen could type, and no ant would ask anyone about it. The
    // dialog has no way out but a key, so the board behind it stays out of
    // reach until there is one.
    if key.prompt.asking
        && let Some(typed) = key_prompt::dialog(&ctx, &mut key.draft)
    {
        // Picked up next frame by `decisions::adopt_typed_key`, which is the
        // only place that may hold a key. The dialog closes there too, so it
        // stays open until the client really exists.
        key.prompt.entered = Some(typed);
    }

    hud::top_bar(
        &mut viewport,
        &hud::ModelState {
            stats: &readout.stats,
            source: readout.source.0.name(),
            awake: orders.awake.0,
            link: &readout.source.0.link(),
        },
        readout.layer.0,
        colony.stores.0,
        colony.fruits.iter().count(),
    );
    // The order is read here and only written when it really changed — see the
    // note on `bottom_bar`.
    let task = colony.scenario.as_ref().map(|scenario| Task {
        briefing: &scenario.briefing,
        stored: colony.stores.0,
        target: scenario.target,
        solved: colony.solved.0,
    });
    let names: Vec<String> = colony
        .levels
        .0
        .iter()
        .map(|level| level.name.clone())
        .collect();
    let playing = colony
        .scenario
        .as_ref()
        .map(|scenario| scenario.name.clone());

    let bar = queen_input::bottom_bar(
        &mut viewport,
        &mut orders.draft,
        &orders.order.0,
        orders.awake.0,
        task.as_ref(),
        &queen_input::LevelList {
            names: &names,
            playing: playing.as_deref(),
        },
        &mut orders.history,
    );
    match bar.order {
        Some(OrderChange::Say(said)) => orders.order.0 = said,
        Some(OrderChange::Silence) => orders.order.0.clear(),
        None => {}
    }
    // Picked here, started next frame by `level::start_chosen_level` — the UI
    // must not tear the world down from inside its own draw.
    if let Some(picked) = bar.level {
        colony.chosen.0 = Some(match picked {
            None => Choice::OpenField,
            Some(index) => Choice::Task(Box::new(colony.levels.0[index].clone())),
        });
    } else if bar.restart {
        // Restarting is picking what is already running, so it takes the same
        // road and needs no second one. The scenario in force is the truth
        // about which board that is — the picker's highlight is only a
        // rendering of it.
        colony.chosen.0 = Some(match colony.scenario.as_ref() {
            Some(scenario) => Choice::Task(Box::new(Scenario::clone(scenario))),
            None => Choice::OpenField,
        });
    }

    // Both panels have had their say, so what is left of the root `Ui` is the
    // bare board. Everything that maps between world and screen — the camera,
    // the labels, the click that picks an ant — hangs off this one answer.
    let free = viewport.available_rect_before_wrap();
    *colony.view = View {
        scale: View::fit(*colony.grid, Vec2::new(free.width(), free.height())),
        origin: Vec2::new(free.center().x, free.center().y),
        window_centre: Vec2::new(
            ctx.viewport_rect().center().x,
            ctx.viewport_rect().center().y,
        ),
    };

    // Which way is east, on the air around the board. Not behind F3: it tells
    // the player what the words in their own order mean, and that is not debug
    // information.
    hud::compass(&mut viewport, &colony.view, *colony.grid);

    if readout.layer.0 {
        hud::intent_overlay(&mut viewport, &colony.view, &colony.ants);
    }

    // The two windows on top of the board. Both are closable; the bench comes
    // back with F2, the inspector by clicking an ant again.
    let showing = bench.selected.0.and_then(|wanted| {
        colony
            .ants
            .iter()
            .find(|(id, _, _, _)| **id == wanted)
            .and_then(|(_, _, _, exchange)| exchange.0.as_ref())
    });
    inspector::window(&ctx, &mut bench.selected, &mut bench.tab, showing);

    // The text in force is read out as a copy, and `questions` is touched only
    // when something was really committed. Reaching into a `ResMut` to read it
    // risks marking it changed every frame, and `request_decisions` answers a
    // changed question by asking all twenty ants again — sixty times a second.
    // That is the same trap `bottom_bar` documents for the queen's order.
    let in_force = bench.questions.step.clone();
    let reworded = lab::window(
        &ctx,
        &mut bench.open.0,
        &mut bench.draft,
        &in_force,
        &bench.log,
    );
    if let Some(reworded) = reworded {
        bench.questions.step = reworded;
    }

    // Picking comes last, once everything else has had its say about this
    // click. Two tests, and both are needed:
    //
    //   * the bare board is whatever the panels left over, which is exactly
    //     what the root `Ui` still has available after they have been shown;
    //   * a window on top of it owns its own rectangle, and windows live in a
    //     layer above the background one.
    //
    // `egui_wants_pointer_input` would be the obvious call and is the wrong one
    // here: it leans on a rect that only `Context::run` fills in, and bevy_egui
    // drives egui through `begin_pass`/`end_pass` instead. It reports the whole
    // viewport as egui's, and no click would ever reach an ant.
    if ctx.input(|input| input.pointer.primary_clicked())
        && let Some(at) = ctx.pointer_interact_pos()
        && free.contains(at)
        && !ctx
            .layer_id_at(at)
            .is_some_and(|layer| layer.order != egui::Order::Background)
    {
        // The same mapping the labels use, read backwards.
        let world = colony.view.to_world(Vec2::new(at.x, at.y));
        bench.selected.0 = nearest_ant(&colony.ants, world);
    }

    Ok(())
}

/// The ant under the pointer, if the click was close enough to one. A click on
/// bare ground clears the selection rather than keeping a stale window open.
fn nearest_ant(ants: &Ants<'_, '_>, world: Vec2) -> Option<AntId> {
    // The reach is in world units, so it holds whatever the zoom is: three
    // quarters of a cell stays three quarters of a cell.
    ants.iter()
        .map(|(id, transform, _, _)| (*id, transform.translation.truncate().distance(world)))
        .filter(|(_, distance)| *distance <= CELL_SIZE * 0.75)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(id, _)| id)
}
