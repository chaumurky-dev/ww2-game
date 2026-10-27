//! Events produced by [`crate::apply`]: enough to render a log and animate.
use crate::map::LocationId;
use crate::power::{Power, Team};
use crate::state::{BattleKind, Phase, PurchaseItem, VictoryReason};
use crate::tech::Tech;
use crate::units::{UnitId, UnitRef, UnitType};
use serde::{Deserialize, Serialize};

/// What a die roll was for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiceContext {
    /// Research dice.
    Research,
    /// Which technology a breakthrough produced.
    ResearchTech,
    /// Antiaircraft fire.
    AaFire {
        /// Where the gun stands.
        location: LocationId,
    },
    /// Shore bombardment.
    Bombardment {
        /// Battle site.
        location: LocationId,
    },
    /// Submarine first strike.
    FirstStrike {
        /// Battle site.
        location: LocationId,
        /// Round.
        round: u32,
    },
    /// Attacker's regular fire.
    Attack {
        /// Battle site.
        location: LocationId,
        /// Round.
        round: u32,
    },
    /// Defender's fire.
    Defense {
        /// Battle site.
        location: LocationId,
        /// Round.
        round: u32,
    },
    /// Strategic bombing damage.
    StrategicBombing {
        /// Target.
        location: LocationId,
    },
    /// Rocket damage.
    Rocket {
        /// Target.
        location: LocationId,
    },
}

/// One die of a [`Event::DiceRolled`], in the same order as `rolls`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DieDetail {
    /// Unit that rolled (None for research, bombing damage and rockets).
    pub unit: Option<UnitId>,
    /// Type of that unit.
    pub unit_type: Option<UnitType>,
    /// Owner of the die.
    pub owner: Power,
    /// Hits on this value or lower (6 for research, 0 for damage / selection rolls).
    pub hit_on: u8,
    /// The face rolled.
    pub roll: u8,
    /// Whether the die counted as a hit.
    pub hit: bool,
}

/// A die with no unit behind it; `hit_on` 0 means a damage or selection roll that never "hits".
pub(crate) fn plain_die(owner: Power, hit_on: u8, roll: u8) -> DieDetail {
    DieDetail {
        unit: None,
        unit_type: None,
        owner,
        hit_on,
        roll,
        hit: hit_on > 0 && roll <= hit_on,
    }
}

/// How a battle ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleOutcome {
    /// Defenders wiped out, attackers remain.
    AttackerWins,
    /// Attackers wiped out (or both).
    DefenderWins,
    /// Attackers retreated.
    Retreated,
    /// Nobody could hit anybody.
    Stalemate,
    /// All attackers were removed from the battle (withdrawn).
    Withdrawn,
}

/// Something that happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A power's turn started.
    TurnStarted {
        /// Round.
        round: u32,
        /// Power.
        power: Power,
    },
    /// Phase changed.
    PhaseChanged {
        /// Power.
        power: Power,
        /// New phase.
        phase: Phase,
    },
    /// Every die roll.
    DiceRolled {
        /// Rolling power.
        power: Power,
        /// Purpose.
        context: DiceContext,
        /// Values 1..=6.
        rolls: Vec<u8>,
        /// Per-die detail, parallel to `rolls` (empty in old stored events).
        #[serde(default)]
        details: Vec<DieDetail>,
    },
    /// Research purchased.
    ResearchBought {
        /// Power.
        power: Power,
        /// Dice.
        dice: u32,
        /// IPC paid.
        cost: u32,
    },
    /// Technology gained.
    TechGained {
        /// Power.
        power: Power,
        /// Tech.
        tech: Tech,
    },
    /// Purchase list set.
    Purchased {
        /// Power.
        power: Power,
        /// Items.
        items: Vec<PurchaseItem>,
        /// Total cost.
        cost: u32,
    },
    /// Units moved along a path.
    UnitsMoved {
        /// Power.
        power: Power,
        /// Units.
        units: Vec<UnitRef>,
        /// Start.
        from: LocationId,
        /// Locations entered.
        path: Vec<LocationId>,
    },
    /// The player took back some of this phase's movement.
    MovesUndone {
        /// Power.
        power: Power,
        /// Units whose moves were taken back as requested.
        units: Vec<UnitRef>,
        /// Further units put back because their move depended on a removed one.
        cascaded: Vec<UnitRef>,
    },
    /// Units boarded a transport.
    UnitsLoaded {
        /// Power.
        power: Power,
        /// Units.
        units: Vec<UnitRef>,
        /// Transport.
        transport: UnitId,
        /// Sea zone.
        at: LocationId,
    },
    /// Units left a transport.
    UnitsUnloaded {
        /// Power.
        power: Power,
        /// Units.
        units: Vec<UnitRef>,
        /// Transport.
        transport: UnitId,
        /// Destination.
        to: LocationId,
        /// True if the landing is deferred to the Combat phase (amphibious assault).
        assault: bool,
    },
    /// Aircraft landed on a carrier.
    AircraftLanded {
        /// Aircraft.
        unit: UnitRef,
        /// Carrier.
        carrier: UnitId,
    },
    /// AA fire result.
    AaFire {
        /// Gun owner.
        gun_owner: Power,
        /// Where.
        location: LocationId,
        /// Aircraft destroyed.
        destroyed: Vec<UnitRef>,
    },
    /// A neutral was violated.
    NeutralViolated {
        /// Violator.
        power: Power,
        /// Territory.
        location: LocationId,
        /// Fee paid.
        fee: u32,
    },
    /// Territory changed hands.
    TerritoryCaptured {
        /// New controller.
        power: Power,
        /// Territory.
        location: LocationId,
        /// Previous controller.
        from: Option<Power>,
        /// True if this returned the territory to its original ally owner.
        liberated: bool,
    },
    /// A capital was taken: treasury seized.
    CapitalCaptured {
        /// Capturing power.
        power: Power,
        /// Owner of the capital.
        capital_of: Power,
        /// IPC seized.
        ipc_seized: u32,
    },
    /// Territories returned to their original owner when its capital was freed.
    ControlRestored {
        /// Original owner.
        power: Power,
        /// Territories.
        locations: Vec<LocationId>,
    },
    /// A battle began.
    BattleStarted {
        /// Site.
        location: LocationId,
        /// Land or sea.
        kind: BattleKind,
        /// Attacker.
        attacker: Power,
        /// Attacking units.
        attackers: Vec<UnitRef>,
        /// Defending units.
        defenders: Vec<UnitRef>,
        /// Amphibious assault?
        amphibious: bool,
    },
    /// New battle round.
    BattleRound {
        /// Site.
        location: LocationId,
        /// Round.
        round: u32,
    },
    /// Hits scored in a roll.
    HitsScored {
        /// Site.
        location: LocationId,
        /// Who fired.
        by: Power,
        /// Hits.
        hits: u32,
    },
    /// Units destroyed.
    CasualtiesRemoved {
        /// Where.
        location: LocationId,
        /// Units lost.
        units: Vec<UnitRef>,
    },
    /// Bombardment hits were scored.
    Bombardment {
        /// Site.
        location: LocationId,
        /// Hits.
        hits: u32,
    },
    /// Attackers retreated.
    Retreated {
        /// Units.
        units: Vec<UnitRef>,
        /// From.
        from: LocationId,
        /// To.
        to: LocationId,
    },
    /// Submarines withdrew.
    SubsWithdrew {
        /// Subs.
        units: Vec<UnitRef>,
        /// From.
        from: LocationId,
        /// To.
        to: LocationId,
    },
    /// Battle finished.
    BattleEnded {
        /// Site.
        location: LocationId,
        /// Result.
        outcome: BattleOutcome,
    },
    /// An amphibious assault was cancelled because the sea zone was not cleared.
    AssaultCancelled {
        /// Target territory.
        location: LocationId,
    },
    /// Strategic bombing result.
    StrategicBombing {
        /// Bomber owner.
        power: Power,
        /// Target.
        location: LocationId,
        /// Owner of the complex.
        victim: Power,
        /// IPC removed.
        damage: u32,
    },
    /// Rocket attack result.
    RocketFired {
        /// Owner.
        power: Power,
        /// Target.
        location: LocationId,
        /// Victim.
        victim: Power,
        /// IPC removed.
        damage: u32,
    },
    /// Units placed.
    UnitsPlaced {
        /// Power.
        power: Power,
        /// Type.
        kind: UnitType,
        /// Where.
        location: LocationId,
        /// New ids.
        ids: Vec<UnitId>,
    },
    /// Units lost for a non-combat reason (cannot land, unplaceable).
    UnitsLost {
        /// Owner.
        power: Power,
        /// Units.
        units: Vec<UnitRef>,
        /// Explanation.
        reason: String,
    },
    /// Income collected.
    IncomeCollected {
        /// Power.
        power: Power,
        /// IPC added.
        amount: u32,
        /// New treasury.
        treasury: u32,
    },
    /// A power conceded.
    Conceded {
        /// Power.
        power: Power,
    },
    /// The game ended.
    GameOver {
        /// Winning team.
        winner: Team,
        /// Reason.
        reason: VictoryReason,
    },
}
