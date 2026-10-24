use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use wartide_protocol::game::UserRef;
use wartide_protocol::group::*;

use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::state::AppState;
use crate::util::*;

#[derive(FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    slug: String,
    description: Option<String>,
    invite_code: String,
    member_count: i64,
    my_role: Option<String>,
}

fn role_from(s: &str) -> GroupRole {
    match s {
        "owner" => GroupRole::Owner,
        "admin" => GroupRole::Admin,
        _ => GroupRole::Member,
    }
}

fn role_str(r: GroupRole) -> &'static str {
    match r {
        GroupRole::Owner => "owner",
        GroupRole::Admin => "admin",
        GroupRole::Member => "member",
    }
}

impl GroupRow {
    fn into_group(self) -> Group {
        let my_role = self.my_role.as_deref().map(role_from);
        Group {
            id: self.id,
            name: self.name,
            slug: self.slug,
            description: self.description,
            member_count: self.member_count,
            invite_code: my_role.filter(|r| r.can_manage()).map(|_| self.invite_code),
            my_role,
        }
    }
}

const GROUP_SELECT: &str = "SELECT g.id, g.name, g.slug, g.description, g.invite_code,
    (SELECT count(*) FROM group_members m WHERE m.group_id=g.id) AS member_count,
    (SELECT role FROM group_members m WHERE m.group_id=g.id AND m.user_id=$1) AS my_role
    FROM groups g";

pub async fn role_of(state: &AppState, group: Uuid, user: Uuid) -> AppResult<Option<GroupRole>> {
    let r: Option<(String,)> =
        sqlx::query_as("SELECT role FROM group_members WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .fetch_optional(&state.db)
            .await?;
    Ok(r.map(|(s,)| role_from(&s)))
}

pub async fn create(
    state: &AppState,
    user: &CurrentUser,
    req: CreateGroupRequest,
) -> AppResult<Group> {
    let name = req.name.trim().to_string();
    if name.len() < 3 || name.chars().count() > 48 {
        return Err(AppError::bad("group name must be 3-48 characters"));
    }
    let base = slugify(&name);
    let id = Uuid::new_v4();
    let mut slug = base.clone();
    for attempt in 0..5 {
        let mut tx = state.db.begin().await?;
        let r = sqlx::query(
            "INSERT INTO groups (id, name, slug, description, invite_code, owner_id) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(&name)
        .bind(&slug)
        .bind(req.description.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(new_invite_code())
        .bind(user.id)
        .execute(&mut *tx)
        .await;
        match r {
            Ok(_) => {
                sqlx::query(
                    "INSERT INTO group_members (group_id, user_id, role) VALUES ($1,$2,'owner')",
                )
                .bind(id)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return get(state, user.id, id).await;
            }
            Err(e) if is_unique_violation(&e) => {
                slug = format!(
                    "{base}-{}",
                    &Uuid::new_v4().simple().to_string()[..4 + attempt]
                );
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::conflict(
        "slug_taken",
        "could not allocate a unique slug",
    ))
}

pub async fn get(state: &AppState, viewer: Uuid, group: Uuid) -> AppResult<Group> {
    sqlx::query_as::<_, GroupRow>(&format!("{GROUP_SELECT} WHERE g.id=$2"))
        .bind(viewer)
        .bind(group)
        .fetch_optional(&state.db)
        .await?
        .map(GroupRow::into_group)
        .ok_or_else(|| AppError::not_found("group"))
}

pub async fn get_by_slug(state: &AppState, viewer: Uuid, slug: &str) -> AppResult<Group> {
    sqlx::query_as::<_, GroupRow>(&format!("{GROUP_SELECT} WHERE g.slug=$2"))
        .bind(viewer)
        .bind(slug)
        .fetch_optional(&state.db)
        .await?
        .map(GroupRow::into_group)
        .ok_or_else(|| AppError::not_found("group"))
}

pub async fn list_mine(state: &AppState, user: Uuid) -> AppResult<Vec<Group>> {
    Ok(sqlx::query_as::<_, GroupRow>(&format!(
        "{GROUP_SELECT} WHERE EXISTS (SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=$1) ORDER BY g.name"
    ))
    .bind(user)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(GroupRow::into_group)
    .collect())
}

pub async fn rotate_invite(state: &AppState, user: Uuid, group: Uuid) -> AppResult<Group> {
    let role = role_of(state, group, user)
        .await?
        .ok_or_else(|| AppError::not_found("group"))?;
    if !role.can_manage() {
        return Err(AppError::forbidden(
            "only owners and admins can rotate the invite code",
        ));
    }
    sqlx::query("UPDATE groups SET invite_code=$2 WHERE id=$1")
        .bind(group)
        .bind(new_invite_code())
        .execute(&state.db)
        .await?;
    get(state, user, group).await
}

pub async fn join(state: &AppState, user: Uuid, code: &str) -> AppResult<Group> {
    let g: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM groups WHERE invite_code=$1")
        .bind(normalize_invite_code(code))
        .fetch_optional(&state.db)
        .await?;
    let (gid,) = g.ok_or_else(|| AppError::not_found("invite code"))?;
    sqlx::query("INSERT INTO group_members (group_id, user_id, role) VALUES ($1,$2,'member') ON CONFLICT DO NOTHING")
        .bind(gid)
        .bind(user)
        .execute(&state.db)
        .await?;
    get(state, user, gid).await
}

pub async fn leave(state: &AppState, user: Uuid, group: Uuid) -> AppResult<()> {
    let role = role_of(state, group, user)
        .await?
        .ok_or_else(|| AppError::not_found("group"))?;
    if role == GroupRole::Owner {
        return Err(AppError::conflict(
            "owner_cannot_leave",
            "the owner cannot leave; transfer ownership first",
        ));
    }
    sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
        .bind(group)
        .bind(user)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn members(state: &AppState, viewer: Uuid, group: Uuid) -> AppResult<Vec<GroupMember>> {
    role_of(state, group, viewer)
        .await?
        .ok_or_else(|| AppError::forbidden("not a member of this group"))?;
    #[derive(FromRow)]
    struct Rw {
        id: Uuid,
        username: String,
        display_name: String,
        avatar: Option<String>,
        role: String,
        joined_at: DateTime<Utc>,
    }
    let rows = sqlx::query_as::<_, Rw>(
        "SELECT u.id, u.username, u.display_name, u.avatar, m.role, m.joined_at
         FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1
         ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, m.joined_at",
    )
    .bind(group)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| GroupMember {
            user: UserRef {
                id: r.id,
                username: r.username,
                display_name: r.display_name,
                avatar: r.avatar,
            },
            role: role_from(&r.role),
            joined_at: r.joined_at,
        })
        .collect())
}

/// Owner can promote/demote between admin and member; the owner role itself is not transferable here.
pub async fn set_role(
    state: &AppState,
    actor: Uuid,
    group: Uuid,
    target: Uuid,
    role: GroupRole,
) -> AppResult<()> {
    let mine = role_of(state, group, actor)
        .await?
        .ok_or_else(|| AppError::not_found("group"))?;
    if mine != GroupRole::Owner {
        return Err(AppError::forbidden("only the owner can change roles"));
    }
    if role == GroupRole::Owner || target == actor {
        return Err(AppError::bad("cannot assign or change the owner role"));
    }
    let n = sqlx::query("UPDATE group_members SET role=$3 WHERE group_id=$1 AND user_id=$2")
        .bind(group)
        .bind(target)
        .bind(role_str(role))
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("member"));
    }
    Ok(())
}

pub async fn remove_member(
    state: &AppState,
    actor: Uuid,
    group: Uuid,
    target: Uuid,
) -> AppResult<()> {
    let mine = role_of(state, group, actor)
        .await?
        .ok_or_else(|| AppError::not_found("group"))?;
    let theirs = role_of(state, group, target)
        .await?
        .ok_or_else(|| AppError::not_found("member"))?;
    let allowed = match mine {
        GroupRole::Owner => theirs != GroupRole::Owner,
        GroupRole::Admin => theirs == GroupRole::Member,
        GroupRole::Member => false,
    };
    if !allowed {
        return Err(AppError::forbidden("not allowed to remove this member"));
    }
    sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
        .bind(group)
        .bind(target)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Leaderboard for a group ladder (`2p` or `5p`). Members only.
pub async fn leaderboard(
    state: &AppState,
    viewer: Uuid,
    group: Uuid,
    five_player: bool,
) -> AppResult<Vec<LeaderboardEntry>> {
    role_of(state, group, viewer)
        .await?
        .ok_or_else(|| AppError::forbidden("not a member of this group"))?;
    let ladder = crate::services::ratings::ladder_for(Some(group), !five_player);
    leaderboard_for_ladder(state, &ladder, 100).await
}

pub async fn leaderboard_for_ladder(
    state: &AppState,
    ladder: &str,
    limit: i64,
) -> AppResult<Vec<LeaderboardEntry>> {
    #[derive(FromRow)]
    struct Rw {
        id: Uuid,
        username: String,
        display_name: String,
        avatar: Option<String>,
        rating: f64,
        deviation: f64,
        games: i32,
        wins: i32,
        losses: i32,
        rank: i64,
    }
    let rows = sqlx::query_as::<_, Rw>(
        "SELECT u.id, u.username, u.display_name, u.avatar, r.rating, r.deviation, r.games, r.wins, r.losses,
                RANK() OVER (ORDER BY r.rating DESC) AS rank
         FROM ratings r JOIN users u ON u.id=r.user_id WHERE r.ladder=$1 ORDER BY r.rating DESC LIMIT $2",
    )
    .bind(ladder)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| LeaderboardEntry {
            rank: r.rank,
            user: UserRef {
                id: r.id,
                username: r.username,
                display_name: r.display_name,
                avatar: r.avatar,
            },
            rating: r.rating,
            rating_deviation: r.deviation,
            games_played: r.games,
            wins: r.wins,
            losses: r.losses,
        })
        .collect())
}
