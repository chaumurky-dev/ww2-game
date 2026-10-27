# Wartide Rules Reference

Authoritative mechanics specification for the Wartide engine. It describes the mechanics of the
1986 "classic, second edition" five-power WWII board game, written in our own words. The source is
referred to throughout only as **the 1986 classic edition**. Territory names are geographic and used as-is.
Production points are called **IPC**.

Machine-readable companions: `crates/engine/data/map.json` (map, starting units, canals),
`crates/engine/data/units.json` (unit table). `scripts/check_map.py` validates both.

Conventions used below: "must" = the engine must enforce it; "may" = a legal choice offered to the
player; **[Decision]** marks a place where sources disagree or are silent and we picked an
interpretation (collected again in section 13). All die rolls are one six-sided die; "hit on N" means a
roll of N or less scores a hit.

---

## 1. Powers, teams, order, treasuries, capitals

| Order | Power (id) | Team | Capital territory | Starting treasury (IPC) | Starting income |
|---|---|---|---|---|---|
| 1 | Soviet Union (`ussr`) | Allies | Russia | 24 | 24 |
| 2 | Germany (`germany`) | Axis | Germany | 32 | 32 |
| 3 | United Kingdom (`uk`) | Allies | United Kingdom | 30 | 30 |
| 4 | Japan (`japan`) | Axis | Japan | 25 | 25 |
| 5 | United States (`usa`) | Allies | Eastern U.S.A. | 36 | 36 |

* A **turn** is the full sequence of section 3 for one power. A **round** is five turns, USSR through USA.
  Round 1 starts from the printed setup (spring 1942).
* Income equals the summed IPC value of the territories the power controls (verified against the map data:
  24/32/30/25/36). Starting treasury equals starting income.
* Teams: Axis = Germany + Japan. Allies = USSR + UK + USA. Team members are "allies" of one another; everyone
  on the other team is an "enemy".
* A single human may control several powers; each power keeps a separate treasury, units and technologies.
* No IPCs, units or territories may ever be given from one power to another, including between allies.

### 1.1 Victory conditions (checked as described)

**Allied victory (military):** the Allies hold both Axis capitals (Germany and Japan) at the end of a turn.
The game ends immediately after the end of the turn in which this becomes true (after that player's
income step).

**Axis victory, military:** the Axis hold two of the three Allied capitals (Russia, United Kingdom,
Eastern U.S.A.) at the end of a turn. Game ends at the end of that turn.

**Axis victory, economic (part of the standard rules):** at the end of a *round* (after the USA turn is complete) the
two Axis powers' combined production level is **84 IPC or more**. Production level of a power is the summed
IPC value of the territories it currently controls, independent of whether its capital is held
(**[Decision]**: a power whose capital has fallen still counts its territories for this test).

**Optional / house variants (all off by default, engine config flags):**

* *Allied economic victory:* not in the 1986 text. The community game files offer an optional Allied
  threshold of **110** combined production at the end of a round (their default for the flag is off, with a
  placeholder of 140). We provide `allied_economic_victory: Option<u32>`, default `None`, suggested value 110.
* *Total victory:* a military victory additionally requires that the winning side's own capitals are all in
  friendly hands.
* *Individual winner:* when a team wins, the individual winner is the member of the winning team whose
  production level grew by the largest percentage over its starting income (24/32/30/25/36). Optional; the
  official FAQ recommends it as a balance device. Ties are shared.
* *Balance options (official, optional, may be combined, but sources advise one at a time):* Germany starts
  with jet power and Japan with super subs; the USSR may not attack on its first turn (no battles/captures, but
  it moves normally); only the original eight industrial complexes may be used (no new ones).
* *Naval placement in enemy waters:* newly purchased ships may be placed in an enemy-occupied sea zone next to
  a complex you own (your turn then ends; the enemy chooses to retreat or fight).

"Capital held" (by an enemy) means the capital territory is currently controlled by a power of the opposing team.
"Capital controlled" means it is controlled by its owning power (a capital liberated by an ally reverts to the owning power, see 9.3).

---

## 2. Units

| Unit | Cost | Attack (hit on) | Defense (hit on) | Move | Domain |
|---|---|---|---|---|---|
| Infantry | 3 | 1 | 2 | 1 | land |
| Armor (tank) | 5 | 3 | 2 | 2 | land |
| Fighter | 12 | 3 | 4 | 4 | air |
| Bomber | 15 | 4 | 1 | 6 | air |
| Battleship | 24 | 4 | 4 | 2 | sea |
| Aircraft carrier | 18 | 1 | 3 | 2 | sea |
| Transport | 8 | none | 1 | 2 | sea |
| Submarine | 8 | 2 | 2 | 2 | sea |
| Antiaircraft (AA) gun | 5 | none | none (special) | 1 | land |
| Industrial complex | 15 | none | none | none | land structure |

Costs of 12 for a fighter, 15 for a bomber, 18 carrier, 24 battleship, 15 complex are printed on the
reference charts of the 1986 classic edition. Some community write-ups list a cheaper complex; ignore them.

### 2.1 Unit rules

**Infantry.** Moves 1 space. Fights only on land. Can be carried by transports.

**Armor.** Moves up to 2 land spaces. May **blitz** (4.4). Fights only on land. Carried by transports.

**Fighter.** Moves 4 spaces per turn *in total* across the combat-move and non-combat-move phases (the
outbound flight and the return both count). Attacks at 3. Defends at 4 on land, and at sea only while based
on a carrier. May land on any friendly land territory (owned by its power or an ally at the start of this
turn) or on a friendly carrier with a free slot. Cannot be hit by submarines and cannot be placed on a
carrier at purchase.

**Bomber.** Moves 6 spaces total per turn, attacks at 4, defends at 1. Cannot land on carriers. Alone may
conduct **strategic bombing raids** (4.8/5.7). In one battle a bomber either bombs or fights, not both.

**Battleship.** Moves 2. Attacks/defends at 4. In an amphibious assault it may take one **bombardment**
shot at 4 (5.5). Cannot carry anything.

**Aircraft carrier.** Moves 2. Attacks at 1, defends at 3. **Carries up to 2 fighters** (own or allied;
never bombers). Fighters aboard count as defending in the air: when the carrier is attacked, the owner may
assign a hit to a fighter instead of the carrier (but never a hit scored by a submarine, which cannot hit
aircraft).

**Transport.** Moves 2. No attack. Defends at 1 and acts as a hit-absorber in sea battles.
**Capacity (second edition):** exactly one of the following loads: **(a) one or two infantry, (b) one armor,
(c) one AA gun.** A transport may **not** carry an infantry together with an armor, nor an armor and an AA
gun, etc. Loads may include units of allied powers (see 10). Cargo cannot fire, and if the transport is hit
and removed, all cargo is lost. Transports are the only ships that may "bridge" (4.6).

**Submarine.** Moves 2. Attacks/defends at 2. Special rules:
* *First strike:* an **attacking** sub fires before anything else at the start of **every** combat round;
  ships it hits are removed immediately and do not fire back. Defending subs have no first strike; they
  fire in the normal defender step.
* *Cannot hit aircraft*, and aircraft cannot be assigned as casualties to its hits. If the only attackers are aircraft,
  a defending sub cannot fire at all. (Aircraft *can* hit subs.)
* *Withdraw:* both attacking and defending subs may leave a battle after any round (5.6).
* No destroyers or anti-sub devices exist in this edition; nothing cancels first strike.
* Anything that can hit a sub: battleships, carriers, other subs, fighters, bombers. Transports cannot
  attack so cannot hit anything.

**AA gun.** Moves 1 land space; may be carried by a transport (alone, see above). Never attacks, never takes
part in a regular battle as a hitter or as a casualty, cannot be targeted by anything except the loss of its transport. Only
one AA gun may exist in a territory (any owner). It fires only during **enemy combat movement** at enemy aircraft that
**enter or pass over** its territory: one die per aircraft, a roll of **1** destroys that aircraft (attacker chooses
which aircraft die) with no return fire. It does not fire in non-combat movement, does not fire when on a transport,
and is not used for the defender's land-combat rolls. When its territory is captured it is **captured**, not destroyed,
and works for the capturer on later turns. It cannot be moved by the capturer on the turn of capture. If it
stands in an allied territory it is still its owner's gun: it fires for its owner when that territory is attacked. It
can be destroyed only when its transport is sunk.

**Industrial complex.** Cannot move, attack, defend, or be destroyed or removed (not by bombing, not by
capture). Captured intact with its territory. At most one per territory. See 8 for production.

### 2.2 Dice, hits, simultaneity

Every combat unit that fights rolls one die per round. Combat is simultaneous (units hit in a round still
fire in that round) with exactly three exceptions: AA gun fire, submarine first strike, and the removal
of hits taken by attacking units as defenders return fire, all of which are described in section 5.
The player who takes a hit chooses which of his units is lost (subject to the legality limits in 5.8).

---

## 3. Turn sequence (per power)

1. **Develop weapons** (optional): pay 5 IPC per research die. (6.1)
2. **Purchase units**: spend remaining IPC; purchases are held aside until step 6. (6.2)
3. **Combat movement**: declare and move every attacking unit; no battle is resolved yet. (4)
4. **Conduct combat**: resolve all battles in the order the mover chooses, within the constraints of 5.1. (5)
5. **Non-combat movement**: reposition unmoved units, land aircraft. (7)
6. **Mobilize new units**: place the units bought in step 2. (8)
7. **Collect income**: take IPC equal to current production level, unless the capital is held by an enemy. (9)

A turn may be shortened: only the income step is compulsory. If units were bought they must be placed
(unplaceable ones are simply lost, no refund); if units were moved into combat the combat must be resolved.
After step 7 the victory conditions of 1.1 are checked; after the USA turn the economic test is checked
too.

---

## 4. Movement rules

### 4.1 Spaces, adjacency, control

* The board has land territories and sea zones, all called *spaces*. Two spaces are adjacent if they share a
  border (the neighbour lists in `map.json`). The board wraps left-right but not top-bottom.
* A group of small islands is **one** land territory (Hawaiian Islands, Solomon Islands, Caroline Islands,
  East Indies, Philippines, etc.). Unnamed rocks (e.g. Sicily) are scenery and cannot be entered. Inset
  "blow-up boxes" are only bookkeeping aids and create no adjacency.
* Entering the sea zone around an island and then the island itself counts as 2 spaces for air units.
* A land territory is **friendly** to a power if the power or one of its allies controls it; it is **enemy-controlled** if a
  power of the other team controls it (violated neutrals belong to whoever violated them; untouched neutrals are neither).
  A territory is **enemy-occupied** if enemy *fighting units* (infantry, armor, landed aircraft) are in it. AA
  guns and industrial complexes alone do **not** make a territory enemy-occupied. A sea zone is friendly if it holds no enemy ship.
* A sea zone is **hostile** (enemy-occupied) iff it contains at least one enemy ship (any type, including a lone
  transport or sub). Zones adjacent to enemy land are not hostile merely for being so.
* Moving into an enemy-controlled or enemy-occupied territory, or into a hostile sea zone, or violating a neutral, is a
  **combat move** and can be made only in step 3. Moving among friendly spaces is a non-combat move (step 5), although land and
  sea units may also make such moves in step 3 (using up their move).
* **Canal restrictions.**
  * *Panama:* ships may move between the Caribbean and West Panama zones only if the power (or an ally) controlled Panama at the
    start of its turn. Passing costs 1 move like any adjacent step.
  * *Suez:* ships may move between the East Mediterranean and Red Sea zones only if the power's alliance controls both
    Anglo-Egypt Sudan and Syria Iraq at the start of the turn. The two territories are land-adjacent so land and
    air units cross freely; air units ignore the canal rule.
* Land movement is blocked between Gibraltar and Algeria and between Eastern Europe and Turkey; Finland Norway is not
  land-connected to Eastern Europe. Kwangtung does not touch the Japan/Manchuria sea zone (all encoded in the neighbour lists).
  Karelia touches two sea zones (Baltic and "Karelia"/north), so Soviet naval units placed at Karelia may use either.

### 4.2 General sequencing rules

* A unit that moves during combat movement may not move again in non-combat movement, except air units finishing
  their flight (landing). A unit may be in only one battle per turn.
* All combat movement is completed before any battle is resolved; no reinforcements join a battle once combat has begun.
* Every unit that entered a combat space must be listed as part of that battle.
* **[Decision]** Take-backs: while a Combat Move or Non-Combat Move phase is still open, the player may take
  back moves, loads and unloads made in that phase (`UndoMoves`: all, particular units, or everything that
  ended at one location). The phase is restored to its start and the surviving moves are replayed; a move
  that only made sense because of a taken-back one is taken back with it. This is a convenience of online
  play, not a tabletop rule. Because the physical dice cannot be re-rolled, once any move of the phase has
  drawn antiaircraft fire (4.5) nothing in that phase can be taken back: units that were fired on cannot
  take back their move. Ending the phase makes everything final.

### 4.3 Land units

* Infantry and AA guns: 1 space per turn. Armor: 2 spaces. Land units can never enter an enemy-occupied territory except
  as an attack; they stop upon entering enemy-controlled or neutral territory.
* In the first of two steps a tank may pass through a friendly territory (own or ally's), occupied or not.
* Non-combat: land units go to friendly territories only (not enemy-controlled, not neutral). They may enter a territory captured
  earlier this turn if they did not themselves fight or move during combat movement.
* AA guns may move in non-combat to friendly territory; they cannot enter a territory that already has an AA gun.

### 4.4 Blitzing (armor only)

During combat movement an armor unit moving 2 spaces may enter, as its first space, a territory that is
**enemy-controlled but not enemy-occupied** (units like AA guns and industrial complexes there do not count as occupation).
It captures that territory immediately (flip control, adjust incomes; AA gun and complex come along as captured, the
capturer collects the territory's value on later turns) and continues to a second space. The second space may be enemy-occupied
(a battle), enemy-controlled and empty (captured), friendly, or neutral (violation: then it must stop).
It may **not** blitz through an enemy-occupied or neutral first space: entering one stops the tank there (battle or neutral
capture), and it cannot continue. A first space that is friendly makes it an ordinary move.
Entering a captured, empty first space does not need a battle. Only tanks blitz; infantry never do.

### 4.5 Air units

* Range is total spaces from take-off to landing, both phases combined. Each space entered counts 1, whether it is sea or land;
  leaving a carrier's sea zone does not count the zone being left (the fighter is "in" that zone).
* A flight may pass over any space (including enemy-occupied spaces) except **neutral** territories, which can only be
  overflown in combat movement and then violate neutrality (4.9). In non-combat movement neutrals may not be crossed.
  **[Decision]** non-combat flights may cross enemy-controlled land and hostile sea zones without being
  attacked, provided no battle is started; AA guns do not fire at non-combat flights.
* An AA gun fires at enemy aircraft that fly into or over its territory during the enemy's combat movement phase (one die per
  aircraft, a 1 kills it). This happens for every AA-equipped enemy territory an aircraft enters on its outbound path, including the
  target territory (where the gun fires once at the start of the battle, 5.2). It never fires on the return flight in non-combat movement.
* **No kamikaze:** a plane may not launch in combat movement unless some legal landing place is reachable
  within its remaining range under the position at the moment of launch. If during combat the planned landing becomes impossible (carrier
  not moved because the plane died, etc.) the plan simply changes. The player is never forced to carry out an obsolete plan.
* **Landing spots:** any territory controlled by the power or an ally **at the start of the current turn**, or a friendly
  carrier with room (max 2 fighters, counting allied fighters), in range. Never in a territory captured this turn
  (including blitzed ones), never in an enemy-controlled, enemy-occupied or neutral territory. Bombers cannot use carriers.
* Planes that survive combat must land during non-combat movement. An aircraft that cannot reach a landing spot is destroyed.
* **Fighters launching from a carrier** must leave before the carrier moves, from the zone where the carrier started the
  turn. A carrier may not move after a plane has landed on it that turn. A carrier may move on turns where it
  carries fighters that have not flown (the fighter does not extend its range), including an ally's carrier moving with your
  fighters aboard.
* Aircraft sitting in a territory or on a carrier can only be launched by their owner on that owner's turn.

### 4.6 Sea units and transports

* Ships move up to 2 sea zones. A ship may pass through a zone only if it is friendly or empty; if a zone
  holds enemy ships the moving ship must **stop** there and fight (combat movement only; not allowed in non-combat movement).
* Ships cannot enter land. Subs and transports obey the same movement rules as other ships.
* Transports in non-combat movement may only enter friendly zones. In combat movement a transport may move into a hostile zone
  (as cargo carrier/hit absorber for an attack or alone, but see 5.4 for lone transports).
* **Loading:** a land unit standing in a coastal territory may board a transport that is (at that moment) in an adjacent sea zone. This may
  happen before, during or after the transport's own move (so a transport can pick up cargo at several stops along its 2-zone path).
  Boarding together with unloading is the land unit's **entire move**: a land unit that has moved on land this turn cannot board, and a
  unit that boards (even armor, which normally has 2 moves) may not move over land before or after. A loaded transport may stay at sea with its cargo;
  the cargo can unload on a later turn.
* **Unloading:** once a transport has unloaded, *its* move is over for the turn.
  * In combat movement, unloaded units fight in a single target territory (an **amphibious assault**, 5.5); all cargo of one
    transport goes to one territory.
  * In non-combat movement a transport may land its cargo in friendly coastal territories; its (up to two) infantry may go to two different
    territories if both touch the transport's zone.
  * Units cannot unload into enemy-occupied territory in non-combat movement.
* **Bridging:** a transport sitting in a zone adjacent to two coastal territories may move land units from one to the other
  without leaving the zone, up to its normal capacity, even after it has moved earlier in the turn; after bridging it cannot move again.
  A transport cannot bridge allied units belonging to another power's turn.
* **Allied cargo:** your land units board an allied transport during **your** turn (with consent), are carried when the **ally**
  moves the transport on his turn, and unload on your next turn. You cannot use an ally's transport to load and unload within one of your turns.
  Allied fighters may land on your carrier on their turn and remain through your moves.
* Placement of new units on transports or carriers is not allowed; they sit on the map and board later.

### 4.7 Who can attack what

Land units attack only land territories (infantry/armor/AA reach them by land or by unloading from a transport). Ships attack only sea zones, apart from
battleship bombardment in an amphibious assault. Aircraft attack either kind of space within range. Land units riding a transport are cargo: they cannot fire or be hit
separately in a sea battle.

### 4.8 Strategic bombing declaration

A bomber force heading for an enemy industrial complex declares a bombing raid in combat movement. A bomber that bombs
does not also fight in a normal battle that turn. Raids and normal air attacks may target the same territory, with the raids resolved first.

### 4.9 Neutral territories (1986 rules)

There are fourteen neutral territories (list in Appendix A). They are not impassable.
* Neutral territories have IPC value 0 and no defenders ever (they are never "occupied"
  unless an enemy of the mover occupies them after violation).
* The **first** power to move land units into, or fly aircraft over, a neutral territory **violates** it: it pays the bank **3 IPC**
  (once per territory, ever), takes control of the territory (control marker), and the territory ceases to be neutral for everyone.
  Entering is a combat move. Entering land units must stop there (armor cannot blitz through) and there is no
  battle. Aircraft that only pass over do not have to stop, but the fee, the control change and neutrality loss still happen.
* All neutral moves are made in combat movement only. Nothing may enter a neutral territory in non-combat movement.
* **[Decision]** The 3 IPC must be paid from the treasury at the moment of the move (after research and purchases have been paid). If the
  power cannot pay it the move is illegal.
* After violation a former neutral is an ordinary territory of 0 IPC owned by the violator (no original owner); an enemy who later
  occupies it fights as usual, and an ally recapturing it controls it (no "liberation").
* A violated neutral may hold a new complex later (8.3).
* Amphibious assaults and moves via transports into neutral territories also violate neutrality
  (**[Decision]**, consistent with TripleA's data).

---

## 5. Combat

### 5.1 Overview and ordering

1. Finish all combat movement. List every battle (each space containing enemy units that attackers entered, plus bombing raids).
2. The mover resolves battles one at a time in any order **subject to**: (a) a sea battle in the zone adjacent to an amphibious
   target is resolved before that assault; (b) bombing raids on a territory resolve before ordinary air attacks on it.
3. Unresolved units cannot be reinforced; a unit appears in one battle only.
4. Battles are resolved in rounds until one of the end conditions below.

**Battle end conditions:** attacker destroyed (defender holds), defender destroyed (attacker wins), both destroyed (defender holds),
or the attacker retreats (defender holds). Retreat is only an attacker's option (5.6).

### 5.2 Land battle round procedure

1. *Defender's AA fire (before round 1 only):* if the territory has an AA gun and there are attacking aircraft, the defender rolls 1 die per attacking
   aircraft; each 1 destroys one attacking aircraft chosen by the attacker. These aircraft do not fire.
   If AA guns on the flight path fired earlier (during movement), those losses are already taken.
2. *Amphibious bombardment (round 1 only, if applicable):* each eligible battleship fires one shot at 4 (5.5).
3. *Attacker fires:* roll 1 die per attacking unit that can attack: infantry hit on 1, armor 3, fighters 3, bombers 4 (heavy bombers: 3 dice each). The defender picks casualties.
4. *Defender fires:* roll 1 die per defending unit, including those hit this round in steps 2-3 (casualties still fire). Infantry/armor hit on 2, fighters on 4 (5 with jet power), bombers on 1.
   The attacker picks which of his units are lost.
5. Remove all casualties of both sides. If both sides still have units, the attacker may retreat or continue to the next round (repeat from step 3, no AA, no bombardment).
* The AA gun and complex are never rolled for and never casualties; they ride with the territory.
* An AA gun that is the only defender is not a defender (territory not occupied): the attacker captures with no battle.

### 5.3 Capturing territories

When the defenders are wiped out, the attacker takes the territory only if at least one surviving attacking **land** unit is present.
If only aircraft survive, the territory stays under its previous controller (even though empty), and the aircraft must leave in
non-combat movement. On capture:
* control flips, production levels are adjusted, any AA gun and industrial complex change to the capturer (see 8 about using the complex),
* if it was originally owned by an ally of the capturer see liberation (9.3), and if it was an enemy capital see 9.4.
* a territory containing nothing but enemy AA/complex is taken by simply moving land units in (a combat move, no roll).

### 5.4 Naval battle round procedure

1. *Attacking submarines' first strike:* each attacking sub rolls 1 die; a hit (2, or 3 with super subs) makes the defender choose a
   non-air unit (ship) which is removed **immediately** and does not return fire. If the defender has only aircraft (fighters on a
   carrier) and no ship is left to hit, the extra hit is lost. Aircraft are never chosen.
2. *Attacker fires all other units:* battleships on 4, carriers on 1, fighters on 3, bombers on 4. (A fighter that is on
   a carrier as defender is "in the air".) Transports cannot fire. Defender selects casualties (these remain until step 3).
3. *Defender fires all units*, including the ones just selected as casualties but **not** those removed by first strike: battleships 4,
   carriers 3, subs 2, transports 1, fighters 4 (5 jet), bombers 1. Submarines can't select aircraft as targets (their hits against a
   force consisting only of aircraft are lost). Attacker selects casualties; he may choose transports (and their cargo) as casualties.
4. Remove all casualties.
5. *Submarine withdrawal opportunity* (5.6), then the attacker may retreat the rest or continue (repeat from step 1; first strike recurs every round).

* A transport that is alone, or that is the only attacker, cannot attack; it still takes one round of enemy fire before it may retreat.
* If neither side has any unit able to hit anything, the engine ends the battle as a stalemate and both stay.
* Transports in a defending fleet defend at 1 and may be chosen as casualties; cargo is lost with them.
* Defending fighters not on a carrier cannot defend a sea zone; fighters on a nearby island do not help.

### 5.5 Amphibious assaults and shore bombardment

An amphibious assault is a land attack by units unloaded from transports in an adjacent sea zone.
* If the transports' zone contains enemy ships, a **sea battle must be won first** (all enemy ships there destroyed, or enemy
  subs/transports likewise); the attacker's escorting warships and the transports take part in that battle; if the zone is not cleared, the
  assault is cancelled (transports must retreat if they survive). Air units that fought in the sea battle cannot also attack the land.
* Each **battleship** in the assault zone that did not fight in a sea battle this turn fires **one shot at 4** at the beginning of round 1 only
  (bombardment). The defender picks the casualty. Casualties from bombardment still fire back in round 1. Hit land defenders cannot hit ships. A battleship that fought
  a sea battle first loses its bombardment, and you may not keep a battleship out of the sea battle to preserve it.
* The cargo is considered landed before the bombardment; a missed bombardment cannot cancel the assault.
* After unloading, the transports and warships are not targets in the land battle.
* **No retreat:** neither the unloaded units, nor aircraft or land units that join the same assault, may retreat from that land battle.
* Cargo that could not unload (naval battle lost) stays aboard if its transport survives.

### 5.6 Retreats and withdrawals

* Only the attacker may retreat, and only after the defender has fired in a round (any round). If no defenders remain, there is no retreat.
* **Land battle:** all remaining attackers retreat together into **one** adjacent friendly territory from which at least one of them entered. (Aircraft that
  were part of the attack leave with them but are exempt from the "one space" rule: see below.)
* **Sea battle:** all remaining attackers retreat together into one adjacent friendly or empty sea zone from which at least one attacker came; **[Decision]** not into a
  zone that is, or earlier this turn was, the site of any battle.
* **Aircraft:** may not stay if the rest retreat. They are removed from the battle and land in non-combat movement (spending whatever range remains), not in a
  territory captured this turn and not necessarily where the others retreated.
* **Submarine withdrawal:** after a round's casualties are removed, an attacking sub may withdraw even if the battle continues, and several subs may withdraw in different rounds. Subs withdrawing in the same round must all go to the same
  adjacent zone from which an attacking ship came, and may not go to a zone that is/was a battle site this turn. Attacking subs choose before defending subs.
  A defending sub may withdraw to any adjacent sea zone with no enemy ships or aircraft in it (a zone the attackers came from is allowed if now empty of enemies). If an attacking sub is the
  last attacker standing after wiping out the defenders, it cannot withdraw; it is stuck.
* No retreat is possible from an amphibious land assault. All attackers can still retreat from the preliminary sea battle.

### 5.7 Strategic bombing raid

1. Bombers fly to the target territory (it must contain an enemy industrial complex; it need not contain defenders). Defender's AA gun there (if any) rolls
   1 die per bomber; each 1 destroys one bomber (no return fire).
2. Each surviving bomber rolls 1 die (heavy bombers 3 dice); sum all results.
3. The owner of the complex pays the sum to the bank, limited to his current treasury. The complex is never damaged or removed. There is no per-territory cap in this edition.
4. Bombers then must land (non-combat) like other aircraft. Enemy fighters in the territory do not intercept.

Rocket attack (technology): see 6.1.

### 5.8 Casualty selection rules

* The owner of the unit that took the hit chooses what is lost.
* An attacker's casualties, taken from the defender's return fire, may include any of his units in the battle (ships including transports; land and air units).
* Constraints: sub hits and sub first-strike hits may not be assigned to aircraft; an AA gun hit is assigned to aircraft; a hit cannot be assigned to units that are not part of the
  battle (cargo, units not listed, AA guns, complexes). Cargo dies with its transport and is not selected independently.
* Defenders of mixed allegiance (units of several allied powers in the same space) defend together; the allies agree on casualties, otherwise the attacker chooses; each power rolls its own dice.
* Allied units **cannot attack together** from the same space: each power attacks on its own turn.

---

## 6. Phase 1-2 details: technology and purchasing

### 6.1 Develop weapons

* Decide before purchasing. Pay 5 IPC per die to the bank, pay for all dice you want at once, then roll them all. (Industrial technology does not reduce this price.)
* Each die that shows **6** is a breakthrough. For each breakthrough, roll one more die to learn which technology you gain; if you already own it, re-roll until you get a new one.
  Failed dice give nothing and the money is not refunded; partial results do not carry over.
* A technology belongs to the researching power only; it cannot be shared. Effects are immediate, apply to all units of that type already on the board, and last for the rest of the game.

| Roll | Technology | Effect (this edition) |
|---|---|---|
| 1 | Jet power | Your fighters defend at 5 (attack remains 3). |
| 2 | Rockets | Once per turn you may launch a rocket from one of your AA guns that is within **3 spaces** of an enemy industrial complex (not on a transport): roll 1 die, the owner of the complex pays that many IPC to the bank (limited to his treasury). The gun can move before combat and, if it did not fire, also in non-combat. Enemy AA does not fire at rockets. A captured rocket-capable gun becomes an ordinary AA gun unless the capturer also owns rockets, and cannot move the turn it is captured. |
| 3 | Super submarines | Your submarines attack at 3 (defense stays 2). |
| 4 | Long range aircraft | Fighters move 6, bombers move 8. |
| 5 | Industrial technology | Every unit you buy costs 1 IPC less (infantry 2, armor 4, fighter 11, bomber 14, transport 7, submarine 7, carrier 17, battleship 23, AA gun 4, industrial complex 14). Research dice stay at 5. |
| 6 | Heavy bombers | Each bomber attacks with **3 dice** instead of 1 (each at 4) in ordinary battles, and rolls 3 dice per surviving bomber in strategic bombing raids (summed). Defense stays 1 die at 1. |

Technologies stack independently (e.g. long range aircraft plus heavy bombers). Rocket distance is counted in adjacency steps through any spaces.
Rockets are resolved during step 4, once per turn per power, and are not blocked by the intervening territory type.

### 6.2 Purchase units

* Spend IPC to buy any of: infantry, armor, fighter, bomber, transport, submarine, carrier, battleship, AA gun, industrial complex, at the costs in section 2 (minus 1 each with industrial technology).
* Treasury may not go negative. Unspent IPC carry over. Bought units are held aside and placed in step 6; units not placeable at that time are lost without refund.
* A power whose capital is held by an enemy has no treasury (see 9.4) and normally buys nothing.

---

## 7. Non-combat movement (step 5)

* Unmoved units (those that did not move in step 3) may move to friendly spaces only, never into enemy-controlled, enemy-occupied or neutral territory, never into a hostile sea zone.
* **Air units that survived must end the turn on a legal landing spot** (4.5). Aircraft that flew combat missions finish their flight here; remaining flight distance is what is left after the outbound leg.
* Carriers and other ships move to friendly zones; transports may enter friendly zones, load and unload in friendly coastal territories, bridge (4.6). Carriers take on fighters landing within range, subject to the 2-fighter cap.
* AA guns may move 1 space to friendly land territory unless they were captured this turn or (rocket) already fired.
* No new battles may begin.

---

## 8. Mobilize new units (step 6)

### 8.1 Where

* **Land and air units** go in territories containing an industrial complex that you have controlled since the **start of this turn** (not one captured this turn; not an allied complex).
  Aircraft are placed on land, not on carriers; infantry and others are not placed on transports.
* **Naval units** go in a sea zone **adjacent** to such a territory. The zone must be free of enemy units (allied units are fine). If no legal zone exists the ship is lost.
* **AA guns:** at most one per territory; a gun cannot be placed if the territory already has one.
* **New industrial complexes** are placed in any territory you have controlled since the start of the turn (no minimum IPC value is required; neutrals you have violated also qualify, and
  value-0 territories qualify). One complex per territory. Newly placed complexes cannot supply units until your next turn.
* Across the whole game, all powers combined, only **four** newly bought complexes can ever be placed (the supply holds 12 complexes and 8 are on the board at setup);
  once four have been placed, nobody may buy another (official FAQ). Captured complexes do not count toward the four; neither do complexes placed on a territory that is later lost.

### 8.2 How many per complex

* An **original** complex (one of the 8 on the board at setup) owned by its original owner has **no limit** on units placed there per turn.
* Any **other** complex (bought during the game, or captured) allows placement of as many units as the **IPC value of its territory**. A territory valued 0 (or any neutral)
  supports exactly **1** unit per turn. This applies also to an original complex held by a power other than its original owner (e.g. Germany holding Karelia's complex):
  **[Decision]** if the original owner liberates it, it becomes an "original" complex again.
* All unit types placed from a complex count toward its limit; naval units placed in adjacent zones count against the complex they are adjacent to (if a zone touches several of your complexes you choose which one).
* Captured complexes may not be used the turn of capture. A complex held by a power's ally cannot be used by you, even if you liberated the territory
  (the original owner may use it on his next turn), with the exception in 9.3 while its owner's capital is held.

### 8.3 Placement checklist for the engine

For each purchased unit: validate type, destination, ownership-at-turn-start, complex presence, enemy presence (naval), per-complex limit, AA uniqueness, complex cap and the global 4-new-complex limit.
Anything that cannot be legally placed is discarded.

---

## 9. Income, capitals, liberation

### 9.1 Collecting

At the end of your turn, if your capital is controlled by you, collect IPC equal to the summed value of all territories you control (as of that moment) into your treasury.

### 9.2 Territory ownership changes

* Capturing flips control to the capturer immediately; production levels are derived from control at any time.
* Units that you leave in a captured territory defend it; an empty territory you own stays yours (control markers) until an enemy walks in.
* If you move land units into an enemy-controlled territory with no enemy units, you capture it without a fight (a combat move); if you move in with only
  air, you cannot capture.

### 9.3 Liberation

* If you capture a territory whose **original owner is an ally** (not you), you only **liberate** it: control passes to that original owner, who earns the income, and any AA gun
  and complex there are his again.
* **Exception:** if that ally's **capital is held by an enemy**, you (the liberator) take control, collect the income, and may use its complex until the
  capital is liberated; upon liberation of the capital, control returns to the original owners. **[Decision]**: implemented as a stand-in flag on the territory recording the original owner.
* If an ally captured a territory from an enemy and the enemy then retakes it, and you retake it again, **you** control it, not the ally who captured it first
  (this applies when the territory's original owner is an enemy of yours or has no owner, i.e. nobody is being liberated).
* A territory with no original owner (a violated neutral) is never liberated, only captured.
* You cannot capture a territory controlled by an ally.

### 9.4 Capital capture

* When a power captures an enemy capital it immediately takes **all IPCs in that power's treasury** (and in addition the capital's value and its other conquests count via ownership).
* The power whose capital fell remains in the game: it may still move and fight, but it cannot collect income from any territory and cannot buy
  (its treasury is 0) until its capital is liberated by itself or an ally.
* On liberation it does **not** get back its IPCs and does **not** collect on the liberating turn; it collects normally at the end of its own next turn.
* Only if it captured an enemy capital while its own capital is held does it have IPCs to spend that turn (buying and placing at complexes it controls).
* Captured IPCs are not returned when a capital is recaptured; if a capital changes hands between enemy powers, the new capturer takes whatever is in that treasury (normally 0).

---

## 10. Cooperation between allies

* Allies may enter, pass through, end moves in, and base aircraft in each other's friendly territories and sea zones; stack in the same space; share carriers (each carrier holds up to 2 fighters from any allied powers).
* Allied units defend together but never attack together; each power fires its own units, rolling its own dice.
* Allies may not trade IPCs, units, territories, or the use of industrial complexes (except 9.3). A power cannot capture a territory its ally controls.
* Fighters landed on an ally's carrier cannot take off on the ally's turn; they move with the carrier and fight only as defenders. If the carrier moves in to attack, your fighters do not join but can be offered as losses by agreement.
* Ally-owned AA guns in a territory fire for the **owner of the gun**.
* Transport rules for allies are in 4.6.

---

## 11. Combat and rules edge cases (table)

| Situation | Resolution |
|---|---|
| Defending transports | Defend at 1 and may be chosen as casualties; cargo lost with them. |
| Unescorted attacking transport | Cannot attack; takes one round of fire before retreating; if destroyed cargo is lost. |
| Sub vs. aircraft | Sub cannot hit aircraft; aircraft can hit it; if only air attacks a sub, it never fires back and should withdraw. |
| Sub first strike | Attacking subs only, every round; casualties don't return fire; cannot target aircraft (fighters on a carrier: carrier must take the hit). |
| Fighter on carrier hit by subs only | Carrier is lost, fighter survives and counter-fires at 4 then must land in the same zone (another carrier with space, or friendly island there), else lost. |
| Carrier sunk, fighters survive | Fighters must land in the same sea zone (carrier slot or friendly island); if impossible, destroyed. Allied fighters aboard defend with their own dice and may be hit. |
| Fighters that cannot land | Destroyed at the end of non-combat movement. |
| Attacker clears the zone with a sub | That sub cannot withdraw afterwards. |
| Multi-round battle with bombardment | Bombardment only before round 1; later rounds are plain. |
| Bombardment hit units | Still return fire in round 1. |
| Land battle with only AA gun/complex defenders | No battle; walk in and capture (armor may also blitz through). |
| AA fire with strategic bombing | AA fires once, 1 die per bomber, 1 kills; then bombers roll damage. |
| Bomber hit by AA en route | Each AA gun entered/overflown fires once per plane per entry. |
| Territory with only enemy aircraft | A battle; aircraft defend at 4 (fighter) or 1 (bomber). |
| Air-only victory over land units | Territory not captured; aircraft leave. |
| Defenders retreat? | Never. Only attackers can retreat (and subs withdraw). |
| Who may submerge/withdraw | Attacking and defending subs may withdraw; nothing else. No "submerge" exists in this edition apart from withdrawal. |
| Tech stacking | Technologies are independent and cumulative. |
| Capturing a capital with weapons tech | Tech is a property of the power, not the capital; a captured power keeps its technologies. Captured rockets are plain AA guns. |
| Capital lost: units purchased earlier | Purchases are placed in the same turn so none are in limbo; after capital loss the power's treasury is 0. |
| Capital lost: income | Zero until the capital is liberated and then collects next own turn. |
| Industrial complex limit | New/captured: IPC value of territory (min 1); original: unlimited (while with the original owner). |
| Moving through enemy AA/complex by tank | Not enemy-occupied; blitz allowed. |
| AA gun capture timing | Cannot be moved on the capture turn. |
| Land units after capture | May be joined in non-combat by unmoved land units. |
| Territory flipped by blitz then battle in next | Captured first space stays captured. |
| Aircraft landing in just captured territory | Forbidden. |
| Hostile sea zone | A zone is hostile iff an enemy ship (of any type) is present. Allied ships there do not make it safe if an enemy ship is also present. |

---

## 12. Engine implementation notes

* Randomness: injected seeded RNG; every die roll is logged for replay.
* Round/turn bookkeeping: `turn_start_control` snapshot of who controlled each territory at the start of the turn (needed for landing and placement rules).
* Keep per-unit flags: moved-this-turn, remaining movement, loaded-onto, owner, original power for allied cargo, "captured this turn" for AA guns.
* The attacker is always the active power. Battles can include several defending allied powers.
* Treat unit supply limits (stack/piece counts) as not enforced, except the 4-new-complex limit.

---

## 13. Ambiguities and chosen interpretations

1. **Heavy bombers.** The 1986 supplemental rules sheet says each heavy bomber rolls **three dice** (all counting) in combat and in raids. Later editions use two dice and take the best. We use three.
2. **Transport capacity.** 1986: one tank *or* up to two infantry *or* one AA gun. Later editions allow one tank plus one infantry. We use the 1986 rule.
3. **Submarines.** 1986: attacking-only first strike each round, withdraw for both sides, no destroyers; later editions add surprise strike negation by destroyers, submerging, etc. We follow 1986.
4. **Neutrals.** 1986: enterable by anyone for a one-time 3 IPC fee, instant capture, and the territory loses its neutrality. Later editions make neutrals impassable or "pro-Allied". We follow 1986.
5. **AA guns.** Fire only against aircraft entering/overflying during enemy combat movement, 1 die per plane. The printed reference chart lists a defense number of 1 for the AA gun, but the text says they are not combatants; we treat them as non-combatants. Community map data (TripleA classic) marks AA guns and complexes as destroyed on capture; the rulebook says captured, we capture.
6. **Soviet first-turn restriction.** Appears in community game files as a default; in the rulebook it is only an optional balance variant. Default off.
7. **Allied economic victory (110 / 140).** Not part of the 1986 rules. Optional config only.
8. **Economic victory test.** Axis 84, checked at the end of the USA turn; we count territories irrespective of capital status.
9. **Retreat destination for ships.** The rulebook speaks of "friendly or unoccupied" for naval retreat and only forbids "battle site" targets in the sub clarification. We apply the battle-site restriction to all naval retreats.
10. **Neutral violation without funds.** Not specified; we make the move illegal.
11. **Non-combat flights over enemy land/sea.** Not explicit. We allow them (no fire, no combat) but forbid crossing neutrals.
12. **Capital capture simultaneity.** "Two of three" and "both Axis" are tested as current control at the end of each turn; capturing and later losing a capital does not bank progress.
13. **Original-complex status after capture.** The sheet says a captured original complex gives limited capacity. We make it unlimited again when it returns to its original owner.
14. **Gulf of Mexico adjacency.** The clarification sheet says the two Gulf sea zones touch the Mexico sea zone via the board wrap; the community map has only one Gulf zone. We added the single edge Gulf of Mexico - Mexico sea zones, and did not model the second zone.
15. **Rocket range.** "3 spaces" is counted along adjacency (land and sea spaces both count).
16. **IC production limit for value-0 territory.** One unit per turn.
17. **Amphibious assault on neutrals.** Allowed, with the usual 3 IPC violation.
18. **Territory names/ids.** We use the rulebook names (e.g. Anglo-Egypt Sudan, Syria Iraq) rather than community map names (Anglo Sudan Egypt, Syria Jordan).
## Appendix A. Map summary (authoritative data is `crates/engine/data/map.json`)

Territory values (IPC) by original owner. `*` = capital, `IC` = starts with an industrial complex.

**Soviet Union** (income 24): Russia 8* IC; Caucasus 3; Karelia S.S.R. 3 IC; Evenki National Okrug 2; Kazakh S.S.R. 2; Novosibirsk 2; Soviet Far East 2; Yakut S.S.R. 2

**Germany** (income 32): Germany 10* IC; Southern Europe 6 IC; Western Europe 6; Eastern Europe 3; Ukraine S.S.R. 3; Finland Norway 2; Algeria 1; Libya 1

**United Kingdom** (income 30): United Kingdom 8* IC; Eastern Canada 3; India 3; Anglo-Egypt Sudan 2; Australia 2; South Africa 2; Congo 1; French Equatorial Africa 1; French West Africa 1; Italian East Africa 1; Kenya-Rhodesia 1; Madagascar 1; New Zealand 1; Persia 1; Syria Iraq 1; Western Canada 1; Gibraltar 0

**Japan** (income 25): Japan 8* IC; French Indo China 3; Kwangtung 3; Manchuria 3; Philippines 3; East Indies 2; Borneo Celebes 1; New Guinea 1; Okinawa 1; Caroline Islands 0; Solomon Islands 0; Wake Island 0

**United States** (income 36): Eastern U.S.A. 12* IC; Western U.S.A. 10 IC; Brazil 3; Alaska 2; China 2; Mexico 2; Sinkiang 2; Cuba 1; Hawaiian Islands 1; Panama 1; Midway 0

**Neutral territories (value 0, start unowned):** Afghanistan, Angola, Argentina-Chile, Columbia, Eire, Mongolia, Mozambique, Peru, Rio del Oro, Saudi Arabia, Spain, Sweden, Switzerland, Turkey.

**Island territories (no land neighbours):** Australia, Borneo Celebes, Caroline Islands, Cuba, East Indies, Eire, Hawaiian Islands, Japan, Madagascar, Midway, New Guinea, New Zealand, Okinawa, Philippines, Solomon Islands, United Kingdom, Wake Island.

**Sea zones (58):** Alaska Sea Zone, Angola Sea Zone, Antarctic Sea Zone, Baltic Sea Zone, Black Sea Zone, Borneo Sea Zone, Caribbean Sea Zone, Caroline Islands Sea Zone, Caspian Sea Zone, Central Mediterranean Sea Zone, Congo Sea Zone, East Argentina Sea Zone, East Canada Sea Zone, East Compass Sea Zone, East Indies Sea Zone, East Mediterranean Sea Zone, East Pacific Sea Zone, East US Sea Zone, French Indo China Sea Zone, Gulf of Mexico Sea Zone, Hawaii Sea Zone, Indian Ocean Sea Zone, Japan Sea Zone, Karelia Sea Zone, Kwangtung Sea Zone, Mexico Sea Zone, Midway Sea Zone, Mozambique Sea Zone, New Guinea Sea Zone, New Zealand Sea Zone, North Sea Zone, North Atlantic Sea Zone, North Australia Sea Zone, North Brazil Sea Zone, North Pacific Sea Zone, Okinawa Sea Zone, Peru Sea Zone, Philippines Sea Zone, Red Sea Zone, Solomon Islands Sea Zone, South Africa Sea Zone, South Argentina Sea Zone, South Atlantic Sea Zone, South Australia Sea Zone, South Brazil Sea Zone, South Compass Sea Zone, South East Madagascar Sea Zone, South Pacific Sea Zone, Soviet Far East Sea Zone, Wake Island Sea Zone, West Africa Sea Zone, West Australia Sea Zone, West Canada Sea Zone, West Compass Sea Zone, West Mediterranean Sea Zone, West Panama Sea Zone, West Spain Sea Zone, West US Sea Zone.

Notes: sea zone ids are `sz_` + a snake_case name; sea-zone names come from the community map data and are only labels (the physical board in the 1986 classic edition has unnamed zones for much of the open ocean). The board wraps left-right (Western Canada/Eastern Canada, Western USA/Eastern USA, Mexico/Panama and the sea zones on each edge) but not top-bottom; this is already encoded in the neighbour lists.

## Appendix B. Starting units (also in `map.json` `starting_units`)

**Soviet Union** (starting treasury 24 IPC)

- Caucasus: 5 infantry
- Evenki National Okrug: 2 infantry
- Karelia S.S.R.: 1 aa_gun, 1 armor, 1 fighter, 1 industrial_complex, 3 infantry
- Russia: 1 aa_gun, 2 armor, 1 fighter, 1 industrial_complex, 4 infantry
- Soviet Far East: 1 armor, 2 infantry
- Karelia Sea Zone: 1 submarine, 1 transport
- Yakut S.S.R.: 3 infantry

**Germany** (starting treasury 32 IPC)

- Algeria: 1 infantry
- Eastern Europe: 1 armor, 1 fighter, 3 infantry
- Finland Norway: 1 armor, 1 fighter, 3 infantry
- Germany: 1 aa_gun, 2 armor, 1 bomber, 1 fighter, 1 industrial_complex, 4 infantry
- Libya: 1 armor, 1 infantry
- Southern Europe: 1 aa_gun, 1 armor, 1 industrial_complex, 2 infantry
- Baltic Sea Zone: 1 submarine, 1 transport
- Central Mediterranean Sea Zone: 1 battleship, 1 transport
- West Spain Sea Zone: 1 submarine
- Ukraine S.S.R.: 2 armor, 1 fighter, 3 infantry
- Western Europe: 1 aa_gun, 2 armor, 1 fighter, 2 infantry

**United Kingdom** (starting treasury 30 IPC)

- Anglo-Egypt Sudan: 1 armor, 1 infantry
- Australia: 2 infantry
- Eastern Canada: 1 armor
- India: 1 fighter, 2 infantry
- South Africa: 1 infantry
- Syria Iraq: 1 infantry
- East Canada Sea Zone: 1 transport
- East Mediterranean Sea Zone: 1 submarine
- Indian Ocean Sea Zone: 1 transport
- North Sea Zone: 1 battleship, 1 transport
- West Mediterranean Sea Zone: 1 battleship
- United Kingdom: 1 aa_gun, 1 armor, 1 bomber, 2 fighter, 1 industrial_complex, 2 infantry
- Western Canada: 1 infantry

**Japan** (starting treasury 25 IPC)

- Borneo Celebes: 1 infantry
- Caroline Islands: 1 infantry
- East Indies: 1 infantry
- French Indo China: 1 fighter, 2 infantry
- Japan: 1 aa_gun, 1 armor, 1 bomber, 1 fighter, 1 industrial_complex, 3 infantry
- Kwangtung: 2 infantry
- Manchuria: 1 fighter, 3 infantry
- New Guinea: 1 infantry
- Okinawa: 1 infantry
- Philippines: 1 fighter, 2 infantry
- Solomon Islands: 1 infantry
- Caroline Islands Sea Zone: 1 battleship, 1 carrier, 1 fighter
- Japan Sea Zone: 1 battleship, 1 transport
- Philippines Sea Zone: 1 transport
- Solomon Islands Sea Zone: 1 submarine
- Wake Island: 1 infantry

**United States** (starting treasury 36 IPC)

- Alaska: 1 infantry
- China: 1 fighter, 2 infantry
- Eastern U.S.A.: 1 aa_gun, 1 armor, 1 bomber, 1 fighter, 1 industrial_complex, 2 infantry
- Hawaiian Islands: 1 infantry
- Midway: 1 infantry
- Sinkiang: 2 infantry
- East US Sea Zone: 1 transport
- Hawaii Sea Zone: 1 carrier, 1 fighter, 1 submarine
- West US Sea Zone: 1 battleship, 1 transport
- Western U.S.A.: 1 aa_gun, 1 fighter, 1 industrial_complex, 2 infantry


## Appendix C. Canals (also in `map.json` `canals`)

* Suez Canal: zones `sz_east_mediterranean` and `sz_red`; land territories `anglo_egypt_sudan`, `syria_iraq`. Passable by a power's ships only if its alliance controls both land territories at the start of its turn.
* Panama Canal: zones `sz_caribbean` and `sz_west_panama`; land territory `panama`. Passable only if the moving power or an ally controls Panama at the start of its turn.

## Sources

* The 1986 classic edition rule booklet, second edition (PDF, read via OCR): https://www.axisandallies.org/files/rules/Axis-Allies-2nd-Edition.pdf
* Official FAQ for the second edition rules (Larry Harris / Krieghund): https://www.axisandallies.org/files/AA-Classic-FAQ.pdf
* Publisher's supplemental rules clarification sheet (scan, 1991): http://smo63.fatcow.com/pdf/ClassicSuppDoc.pdf
* Electronic copy of the same clarification sheet: https://www.axisandallies.net/public/history/1997/aaclar.htm
* Index of classic-edition rulebooks and official clarifications: https://www.axisandallies.org/forums/topic/32564/classic-edition-s-rulebooks-and-official-clarifications
* Community map/game file (territories, adjacency, values, starting units, canals, options and notes on the 84 IPC economic victory and the optional Allied threshold): https://github.com/triplea-maps/world_war_ii_classic/blob/master/map/games/classic.xml
* Rules summary wiki: https://en.wikibooks.org/wiki/Axis_&_Allies/Classic
* Classic edition resource page: https://www.axisandallies.org/resources-downloads/axis-allies-2nd-edition/
