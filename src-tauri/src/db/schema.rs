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
        Migration {
            version: 4,
            description: "create_memories_table",
            sql: "
                CREATE TABLE IF NOT EXISTS Memories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    hash TEXT NOT NULL UNIQUE,
                    date TEXT NOT NULL,
                    status TEXT NOT NULL
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "create_media_chunks_table",
            sql: "
                CREATE TABLE IF NOT EXISTS MediaChunks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    memory_id INTEGER NOT NULL,
                    url TEXT NOT NULL,
                    overlay_url TEXT,
                    order_index INTEGER NOT NULL,
                    FOREIGN KEY (memory_id) REFERENCES Memories(id)
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "create_export_jobs_table",
            sql: "
                CREATE TABLE IF NOT EXISTS ExportJobs (
                    id TEXT PRIMARY KEY,
                    created_at DATETIME,
                    status TEXT
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 7,
            description: "add_memories_job_and_output_fields",
            sql: "
                ALTER TABLE Memories ADD COLUMN job_id TEXT;
                ALTER TABLE Memories ADD COLUMN mid TEXT;
                ALTER TABLE Memories ADD COLUMN content_hash TEXT;
                ALTER TABLE Memories ADD COLUMN relative_path TEXT;
                ALTER TABLE Memories ADD COLUMN thumbnail_path TEXT;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_content_hash ON Memories(content_hash);
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 8,
            description: "create_processed_zips_table",
            sql: "
                CREATE TABLE IF NOT EXISTS ProcessedZips (
                    job_id TEXT,
                    filename TEXT,
                    status TEXT,
                    PRIMARY KEY (job_id, filename)
                );
            ",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "add_memory_item_datetime_and_location_resolved",
            sql: "
                ALTER TABLE MemoryItem ADD COLUMN date_time TEXT;
                ALTER TABLE MemoryItem ADD COLUMN location_resolved TEXT;
            ",
            kind: MigrationKind::Up,
        },
    ]
}