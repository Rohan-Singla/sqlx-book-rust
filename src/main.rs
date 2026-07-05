mod errors;

use errors::{AppError, Book};
use serde::Deserialize;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;

#[derive(Deserialize)]
struct BookImport {
    title: String,
    author: String,
    genre: String,
    year: Option<i32>,
}

async fn init_db() -> Result<PgPool, AppError> {
    dotenvy::dotenv_override().ok();
    let url = env::var("DATABASE_URL")
        .map_err(|_| AppError::Message("DATABASE_URL not set".to_string()))?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

async fn add_book(
    pool: &PgPool,
    title: &str,
    author: &str,
    genre: &str,
    year: Option<i32>,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO books (title, author, genre, year) VALUES ($1, $2, $3, $4)")
        .bind(title)
        .bind(author)
        .bind(genre)
        .bind(year)
        .execute(pool)
        .await?;

    println!("Added: \"{}\" by {}", title, author);
    Ok(())
}

async fn list_books(
    pool: &PgPool,
    unread_only: bool,
    sort_by: &str,
    descending: bool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<(), AppError> {
    let order_col = match sort_by {
        "year" => "year",
        "rating" => "rating",
        "author" => "author",
        "genre" => "genre",
        _ => "id",
    };

    let mut books: Vec<Book> = if unread_only {
        sqlx::query_as::<_, Book>(
            "SELECT id, title, author, genre, year, read, rating FROM books WHERE read = 0 ORDER BY id",
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Book>(
            "SELECT id, title, author, genre, year, read, rating FROM books ORDER BY id",
        )
        .fetch_all(pool)
        .await?
    };

    books.sort_by(|a, b| {
        let cmp = match order_col {
            "year" => a.year.cmp(&b.year).then(a.title.cmp(&b.title)),
            "rating" => a.rating.cmp(&b.rating).then(a.title.cmp(&b.title)),
            "author" => a.author.cmp(&b.author).then(a.title.cmp(&b.title)),
            "genre" => a.genre.cmp(&b.genre).then(a.title.cmp(&b.title)),
            _ => a.id.cmp(&b.id),
        };
        if descending { cmp.reverse() } else { cmp }
    });

    let start = offset.unwrap_or(0) as usize;
    let end = limit.map(|l| start + l as usize).unwrap_or(books.len());
    let page: Vec<&Book> = books.iter().skip(start).take(end.saturating_sub(start)).collect();

    if page.is_empty() {
        println!("No books found.");
    } else {
        for book in &page {
            println!("{}", book);
        }
        println!("\n{} book(s) shown (total: {})", page.len(), books.len());
    }

    Ok(())
}

async fn search_books(pool: &PgPool, query: &str) -> Result<(), AppError> {
    let pattern = format!("%{}%", query);

    let books = sqlx::query_as::<_, Book>(
        "SELECT id, title, author, genre, year, read, rating FROM books
         WHERE title ILIKE $1 OR author ILIKE $1 OR genre ILIKE $1
         ORDER BY id",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    if books.is_empty() {
        println!("No books matching \"{}\".", query);
    } else {
        for book in &books {
            println!("{}", book);
        }
        println!("\n{} book(s) found", books.len());
    }

    Ok(())
}

async fn search_by_author(pool: &PgPool, author: &str) -> Result<(), AppError> {
    let pattern = format!("%{}%", author);

    let books = sqlx::query_as::<_, Book>(
        "SELECT id, title, author, genre, year, read, rating FROM books
         WHERE author ILIKE $1 ORDER BY year DESC",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    if books.is_empty() {
        println!("No books by author matching \"{}\".", author);
    } else {
        for book in &books {
            println!("{}", book);
        }
        println!("\n{} book(s) found", books.len());
    }

    Ok(())
}

async fn search_by_genre(pool: &PgPool, genre: &str) -> Result<(), AppError> {
    let pattern = format!("%{}%", genre);

    let books = sqlx::query_as::<_, Book>(
        "SELECT id, title, author, genre, year, read, rating FROM books
         WHERE genre ILIKE $1 ORDER BY year DESC",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    if books.is_empty() {
        println!("No books in genre matching \"{}\".", genre);
    } else {
        for book in &books {
            println!("{}", book);
        }
        println!("\n{} book(s) found", books.len());
    }

    Ok(())
}

// ---------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------

async fn genre_stats(pool: &PgPool) -> Result<(), AppError> {
    let rows = sqlx::query(
        "SELECT genre, COUNT(*) as total, SUM(read) as read_count, AVG(rating::FLOAT8) as avg_rating
         FROM books GROUP BY genre ORDER BY COUNT(*) DESC, genre ASC",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        println!("No books in the library.");
        return Ok(());
    }

    println!("{:<20} {:>6} {:>6} {:>10}", "Genre", "Books", "Read", "Avg Rating");
    println!("{}", "-".repeat(46));

    for row in &rows {
        let genre: &str = row.try_get("genre")?;
        let total: i64 = row.try_get("total")?;
        let read_count: Option<i64> = row.try_get("read_count")?;
        let avg_rating: Option<f64> = row.try_get("avg_rating")?;
        let avg = avg_rating.map(|r| format!("{:.1}", r)).unwrap_or_else(|| "  -".to_string());
        println!("{:<20} {:>6} {:>6} {:>10}", genre, total, read_count.unwrap_or(0), avg);
    }

    Ok(())
}

async fn author_stats(pool: &PgPool) -> Result<(), AppError> {
    let rows = sqlx::query(
        "SELECT author, COUNT(*) as total, SUM(read) as read_count,
                MIN(year) as first_published, MAX(year) as latest_published
         FROM books GROUP BY author HAVING COUNT(*) > 0 ORDER BY COUNT(*) DESC, author ASC",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        println!("No books in the library.");
        return Ok(());
    }

    println!("{:<20} {:>6} {:>6} {:>14} {:>14}", "Author", "Books", "Read", "First Pub", "Latest Pub");
    println!("{}", "-".repeat(66));

    for row in &rows {
        let author: &str = row.try_get("author")?;
        let total: i64 = row.try_get("total")?;
        let read_count: Option<i64> = row.try_get("read_count")?;
        let first: Option<i32> = row.try_get("first_published")?;
        let latest: Option<i32> = row.try_get("latest_published")?;
        let first_str = first.map(|y| y.to_string()).unwrap_or_else(|| "?".to_string());
        let latest_str = latest.map(|y| y.to_string()).unwrap_or_else(|| "?".to_string());
        println!("{:<20} {:>6} {:>6} {:>14} {:>14}", author, total, read_count.unwrap_or(0), first_str, latest_str);
    }

    Ok(())
}

async fn show_counts(pool: &PgPool) -> Result<(), AppError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books")
        .fetch_one(pool)
        .await?;

    let read_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE read = 1")
        .fetch_one(pool)
        .await?;

    let has_ratings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE rating IS NOT NULL")
        .fetch_one(pool)
        .await?;

    let avg_rating: Option<f64> =
        sqlx::query_scalar("SELECT AVG(rating::FLOAT8) FROM books WHERE rating IS NOT NULL")
            .fetch_one(pool)
            .await?;

    println!("Total books:     {}", total);
    println!(
        "Read:            {} ({}%)",
        read_count,
        if total > 0 { (read_count * 100) / total } else { 0 }
    );
    println!("Unread:          {}", total - read_count);
    println!("With ratings:    {}", has_ratings);
    if let Some(avg) = avg_rating {
        println!("Average rating:  {:.1} / 5", avg);
    }

    Ok(())
}


async fn mark_book(pool: &PgPool, id: i32, read: bool) -> Result<(), AppError> {
    let read_val: i32 = if read { 1 } else { 0 };
    let result = sqlx::query("UPDATE books SET read = $1 WHERE id = $2")
        .bind(read_val)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        println!("No book found with ID {}.", id);
    } else {
        println!("Marked book {} as {}.", id, if read { "read" } else { "unread" });
    }

    Ok(())
}

async fn rate_book(pool: &PgPool, id: i32, rating: i32) -> Result<(), AppError> {
    if rating < 1 || rating > 5 {
        return Err(AppError::Message("Rating must be between 1 and 5.".to_string()));
    }

    let result = sqlx::query("UPDATE books SET rating = $1 WHERE id = $2")
        .bind(rating)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        println!("No book found with ID {}.", id);
    } else {
        println!("Rated book {} with {} stars.", id, rating);
    }

    Ok(())
}

async fn delete_book(pool: &PgPool, id: i32) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM books WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        println!("No book found with ID {}.", id);
    } else {
        println!("Deleted book {}.", id);
    }

    Ok(())
}

async fn import_books(pool: &PgPool, file_path: &str) -> Result<(), AppError> {
    let content = tokio::fs::read_to_string(file_path).await?;
    let imports: Vec<BookImport> = serde_json::from_str(&content)
        .map_err(|e| AppError::Message(format!("Invalid JSON: {}", e)))?;

    println!("Importing {} books...", imports.len());

    let mut tx = pool.begin().await?;

    for (i, book) in imports.iter().enumerate() {
        sqlx::query("INSERT INTO books (title, author, genre, year) VALUES ($1, $2, $3, $4)")
            .bind(&book.title)
            .bind(&book.author)
            .bind(&book.genre)
            .bind(book.year)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Message(format!("Row {}: {}", i + 1, e)))?;
    }

    tx.commit().await?;

    println!("Successfully imported {} books.", imports.len());
    Ok(())
}

async fn run() -> Result<(), AppError> {
    let pool = init_db().await?;
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "add" => {
            if args.len() < 5 {
                println!("Usage: cargo run -- add <title> <author> <genre> [year]");
                return Ok(());
            }
            let year: Option<i32> = if args.len() > 5 {
                Some(args[5].parse().map_err(|_| AppError::Message("Invalid year.".to_string()))?)
            } else {
                None
            };
            add_book(&pool, &args[2], &args[3], &args[4], year).await?;
        }
        "list" => {
            let mut unread_only = false;
            let mut sort_by = "id";
            let mut descending = false;
            let mut limit: Option<i64> = None;
            let mut offset: Option<i64> = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--unread" => unread_only = true,
                    "--sort" => {
                        i += 1;
                        if i < args.len() {
                            sort_by = &args[i];
                        }
                    }
                    "--desc" => descending = true,
                    "--limit" => {
                        i += 1;
                        if i < args.len() {
                            limit = args[i].parse().ok();
                        }
                    }
                    "--offset" => {
                        i += 1;
                        if i < args.len() {
                            offset = args[i].parse().ok();
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            list_books(&pool, unread_only, sort_by, descending, limit, offset).await?;
        }
        "search" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- search <query>");
                return Ok(());
            }
            search_books(&pool, &args[2]).await?;
        }
        "search-author" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- search-author <author>");
                return Ok(());
            }
            search_by_author(&pool, &args[2]).await?;
        }
        "search-genre" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- search-genre <genre>");
                return Ok(());
            }
            search_by_genre(&pool, &args[2]).await?;
        }
        "read" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- read <id>");
                return Ok(());
            }
            let id: i32 = args[2].parse().map_err(|_| AppError::Message("Invalid ID.".to_string()))?;
            mark_book(&pool, id, true).await?;
        }
        "unread" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- unread <id>");
                return Ok(());
            }
            let id: i32 = args[2].parse().map_err(|_| AppError::Message("Invalid ID.".to_string()))?;
            mark_book(&pool, id, false).await?;
        }
        "rate" => {
            if args.len() != 4 {
                println!("Usage: cargo run -- rate <id> <1-5>");
                return Ok(());
            }
            let id: i32 = args[2].parse().map_err(|_| AppError::Message("Invalid ID.".to_string()))?;
            let rating: i32 = args[3].parse().map_err(|_| AppError::Message("Invalid rating.".to_string()))?;
            rate_book(&pool, id, rating).await?;
        }
        "delete" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- delete <id>");
                return Ok(());
            }
            let id: i32 = args[2].parse().map_err(|_| AppError::Message("Invalid ID.".to_string()))?;
            delete_book(&pool, id).await?;
        }
        "import" => {
            if args.len() != 3 {
                println!("Usage: cargo run -- import <file.json>");
                return Ok(());
            }
            import_books(&pool, &args[2]).await?;
        }
        "genres" => genre_stats(&pool).await?,
        "authors" => author_stats(&pool).await?,
        "stats" => show_counts(&pool).await?,
        _ => {
            println!("Unknown command: {}", args[1]);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Book Library CLI");
    println!();
    println!("Commands:");
    println!("  add <title> <author> <genre> [year]");
    println!("                            Add a new book");
    println!("  list [--unread] [--sort <col>] [--desc] [--limit N] [--offset N]");
    println!("                            List books with optional filters");
    println!("  search <query>            Search by title, author, or genre");
    println!("  search-author <author>    Search by author");
    println!("  search-genre <genre>      Search by genre");
    println!("  read <id>                 Mark a book as read");
    println!("  unread <id>               Mark a book as unread");
    println!("  rate <id> <1-5>           Rate a book (1-5 stars)");
    println!("  delete <id>               Delete a book");
    println!("  import <file.json>        Import books from a JSON array");
    println!("  genres                    Show genre statistics (GROUP BY)");
    println!("  authors                   Show author statistics");
    println!("  stats                     Show library summary");
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
