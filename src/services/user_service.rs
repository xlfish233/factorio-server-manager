use crate::entity::user;
use crate::error::{AppError, AppResult};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use sea_orm::{entity::*, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

#[derive(Debug, Clone)]
pub struct UserPublic {
    pub username: String,
    pub role: String,
    pub email: Option<String>,
}

pub async fn get_by_username(
    db: &DatabaseConnection,
    username: &str,
) -> AppResult<Option<user::Model>> {
    let model = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await?;
    Ok(model)
}

pub async fn check_password(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> AppResult<bool> {
    let Some(model) = get_by_username(db, username).await? else {
        return Ok(false);
    };
    let decoded = BASE64
        .decode(model.password.as_bytes())
        .map_err(|e| AppError::Config {
            msg: format!("invalid stored password encoding: {}", e),
        })?;
    let hashed = std::str::from_utf8(&decoded).map_err(|e| AppError::Config {
        msg: format!("invalid stored password utf8: {}", e),
    })?;
    let ok = bcrypt::verify(password, hashed).unwrap_or(false);
    Ok(ok)
}

pub fn to_public(model: &user::Model) -> UserPublic {
    UserPublic {
        username: model.username.clone(),
        role: model.role.clone(),
        email: model.email.clone(),
    }
}

fn hash_password_base64(plain: &str) -> Result<String, bcrypt::BcryptError> {
    let hashed = bcrypt::hash(plain, bcrypt::DEFAULT_COST)?;
    Ok(BASE64.encode(hashed.as_bytes()))
}

pub async fn list(db: &DatabaseConnection) -> AppResult<Vec<UserPublic>> {
    let list = user::Entity::find().all(db).await?;
    Ok(list.iter().map(to_public).collect())
}

pub async fn add(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
    role: &str,
    email: Option<String>,
) -> AppResult<()> {
    if get_by_username(db, username).await?.is_some() {
        return Err(AppError::BadRequest {
            msg: format!("user {} already exists", username),
        });
    }
    let pw = hash_password_base64(password).map_err(|e| AppError::Config { msg: e.to_string() })?;
    let model = user::ActiveModel {
        username: Set(username.to_string()),
        password: Set(pw),
        role: Set(role.to_string()),
        email: Set(email),
        ..Default::default()
    };
    model.insert(db).await?;
    Ok(())
}

pub async fn remove(db: &DatabaseConnection, username: &str) -> AppResult<()> {
    // cannot delete single admin user
    let admins = user::Entity::find()
        .filter(user::Column::Role.eq("admin"))
        .all(db)
        .await?;
    if admins.len() == 1 && admins[0].username == username {
        return Err(AppError::BadRequest {
            msg: "cannot delete single admin user".into(),
        });
    }
    let res = user::Entity::delete_many()
        .filter(user::Column::Username.eq(username))
        .exec(db)
        .await?;
    if res.rows_affected == 0 {
        return Err(AppError::BadRequest {
            msg: format!("user {} not found", username),
        });
    }
    Ok(())
}

pub async fn change_password(
    db: &DatabaseConnection,
    username: &str,
    old_password: &str,
    new_password: &str,
    new_password_confirmation: &str,
) -> AppResult<bool> {
    if new_password != new_password_confirmation {
        return Err(AppError::BadRequest {
            msg: "Password confirmation incorrect".into(),
        });
    }
    if !check_password(db, username, old_password).await? {
        return Err(AppError::Unauthorized {
            msg: format!("Password for user {} wrong", username),
        });
    }
    let mut model = get_by_username(db, username)
        .await?
        .ok_or_else(|| AppError::BadRequest {
            msg: "user not found".into(),
        })?
        .into_active_model();
    model.password =
        Set(hash_password_base64(new_password)
            .map_err(|e| AppError::Config { msg: e.to_string() })?);
    model.update(db).await?;
    Ok(true)
}
