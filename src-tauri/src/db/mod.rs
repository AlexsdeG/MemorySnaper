use tauri_plugin_sql::{Migration, MigrationKind};

pub fn sqlite_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create_memory_item_table",
            sql: "
                CREATE TABLE IF NOT EXISTS MemoryItem (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    date TEXT NOT NULL,
                    location TEXT,
                    media_url TEXT NOT NULL,
                    overlay_url TEXT,
                    status TEXT NOT NULL
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "create_export_job_table",
            sql: "
                CREATE TABLE IF NOT EXISTS ExportJob (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    status TEXT NOT NULL,
                    total_files INTEGER NOT NULL DEFAULT 0,
                    downloaded_files INTEGER NOT NULL DEFAULT 0
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "add_memory_item_retry_error_fields",
            sql: "
                ALTER TABLE MemoryItem ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE MemoryItem ADD COLUMN last_error_code TEXT;
                ALTER TABLE MemoryItem ADD COLUMN last_error_message TEXT;
            ",
            kind: MigrationKind::Up,
        },
    ]
}
