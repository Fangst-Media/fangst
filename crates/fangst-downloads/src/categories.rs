//! Categories: free-form labels on jobs, plus the ones created ahead of any job.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{Error, Store, now};

/// Category names end up in folder names and other products' APIs; one line of printable text.
const MAX_CATEGORY_CHARS: usize = 255;

fn check(name: &str) -> Result<&str, Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a category needs a name".to_owned()));
    }
    if name.chars().any(char::is_control) || name.chars().count() > MAX_CATEGORY_CHARS {
        return Err(Error::Invalid(format!(
            "a category name is one line of at most {MAX_CATEGORY_CHARS} characters"
        )));
    }
    Ok(name)
}

impl Store {
    /// Every category: those created and those jobs carry, sorted, each once.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn categories(&self) -> Result<Vec<String>, Error> {
        Ok(sqlx::query_scalar(
            "SELECT name FROM categories
             UNION SELECT category FROM jobs WHERE category IS NOT NULL
             ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Creates a category; one that exists already is left as it is.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an empty or multi-line name, or a database error.
    pub async fn create_category(&self, name: &str) -> Result<(), Error> {
        let name = check(name)?;
        sqlx::query("INSERT OR IGNORE INTO categories (name, created_at) VALUES (?, ?)")
            .bind(name)
            .bind(now())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Removes a created category. Jobs keep theirs, so a name a job carries stays listed.
    ///
    /// # Errors
    /// When the database write fails.
    pub async fn delete_category(&self, name: &str) -> Result<(), Error> {
        sqlx::query("DELETE FROM categories WHERE name = ?")
            .bind(name.trim())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Sets the folder each category's new jobs download into, by category name in any case.
    /// A relative folder is inside the job root. Jobs that already have a folder keep it.
    ///
    /// # Panics
    /// If a thread panicked while holding the folder map.
    pub fn set_category_folders(&self, folders: HashMap<String, PathBuf>) {
        *self.category_folders.write().expect("category folders") = folders
            .into_iter()
            .map(|(name, folder)| (name.trim().to_lowercase(), folder))
            .collect();
    }

    pub(crate) fn category_folder(&self, category: &str) -> Option<PathBuf> {
        self.category_folders
            .read()
            .expect("category folders")
            .get(&category.trim().to_lowercase())
            .cloned()
    }

    /// Puts a job in a category, or in none.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown job, [`Error::Invalid`] for a bad name.
    pub async fn set_category(&self, id: i64, category: Option<&str>) -> Result<(), Error> {
        let category = category.map(check).transpose()?;
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query("UPDATE jobs SET category = ? WHERE id = ?")
            .bind(category)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound(id));
        }
        sqlx::query(
            "INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'category', ?)",
        )
        .bind(id)
        .bind(now())
        .bind(category.map_or_else(
            || "category removed".to_owned(),
            |c| format!("category set to {c}"),
        ))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
