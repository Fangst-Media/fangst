//! Categories: free-form labels on jobs, plus the ones created ahead of any job.

use crate::{CategorySeeding, Error, Store, now};

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

    /// A category's own seeding limits; all `None` for a category that sets none or doesn't exist.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn category_seeding(&self, name: &str) -> Result<CategorySeeding, Error> {
        let row: Option<(Option<f64>, Option<i64>)> = sqlx::query_as(
            "SELECT seed_ratio_limit, seed_time_limit_secs FROM categories WHERE name = ?",
        )
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await?;
        Ok(
            row.map_or_else(CategorySeeding::default, |(ratio, time)| CategorySeeding {
                ratio_limit: ratio,
                time_limit_secs: time,
            }),
        )
    }

    /// Sets a category's seeding limits, creating the category when needed. `None` goes back to
    /// the default; a negative value turns that limit off. Jobs already seeding pick the change
    /// up the next time their counts are written.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a bad name or a ratio that is not a number.
    pub async fn set_category_seeding(
        &self,
        name: &str,
        seeding: CategorySeeding,
    ) -> Result<(), Error> {
        let name = check(name)?;
        if seeding.ratio_limit.is_some_and(|r| !r.is_finite()) {
            return Err(Error::Invalid(
                "a ratio limit is a finite number".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO categories (name, created_at, seed_ratio_limit, seed_time_limit_secs)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (name) DO UPDATE SET seed_ratio_limit = ?3, seed_time_limit_secs = ?4",
        )
        .bind(name)
        .bind(now())
        .bind(seeding.ratio_limit)
        .bind(seeding.time_limit_secs)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The seeding limits of a job's category.
    pub(crate) async fn category_seeding_of(&self, id: i64) -> Result<CategorySeeding, Error> {
        let row: Option<(Option<f64>, Option<i64>)> = sqlx::query_as(
            "SELECT categories.seed_ratio_limit, categories.seed_time_limit_secs
             FROM jobs LEFT JOIN categories ON categories.name = jobs.category
             WHERE jobs.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let (ratio, time) = row.ok_or(Error::NotFound(id))?;
        Ok(CategorySeeding {
            ratio_limit: ratio,
            time_limit_secs: time,
        })
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
