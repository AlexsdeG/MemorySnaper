import { type ResolvedLocale } from "@/lib/language";

const enMessages = {
  "app.header.title": "MemorySnaper",
  "app.tabs.downloader": "Downloader",
  "app.tabs.viewer": "Viewer",
  "app.tabs.settings": "Settings",
  "app.header.subtitle": "Phase 1 tab layout scaffold",
  "app.section.downloader": "Downloader section",
  "app.section.viewer": "Viewer section",
  "app.section.settings": "Settings section",

  "settings.card.title": "Settings",
  "settings.card.description": "Configure language, appearance, and download limits.",
  "settings.form.appearance": "Appearance",
  "settings.form.theme.light": "Light",
  "settings.form.theme.system": "System",
  "settings.form.theme.dark": "Dark",
  "settings.form.language": "Language",
  "settings.form.language.system": "System",
  "settings.form.language.en": "English",
  "settings.form.language.de": "Deutsch",
  "settings.form.language.detected": "Detected language: {locale}",
  "settings.form.requestsPerMinute": "Requests per Minute",
  "settings.form.concurrentDownloads": "Concurrent Downloads",
  "settings.form.warning":
    "Warning: Values above 100 RPM or 5 concurrent downloads may trigger throttling.",
  "settings.form.reset.button": "Reset all app data",
  "settings.form.reset.inProgress": "Resetting data...",
  "settings.form.reset.description":
    "Deletes all imported media records, processing state, and app-managed cache data.",
  "settings.form.reset.confirm":
    "This will permanently delete all app-managed media data and reset the local database. Continue?",
  "settings.form.reset.error": "Reset failed. Please try again.",
  "settings.form.startupPage": "Startup page",
  "settings.form.startupPage.system": "System (dynamic)",
  "settings.form.startupPage.downloader": "Downloader",
  "settings.form.startupPage.viewer": "Viewer",

  "downloader.card.title": "Downloader",
  "downloader.card.description": "Import Snapchat export data and download queued memories.",
  "downloader.workflow.upload.title": "Upload Snapchat Export",
  "downloader.workflow.upload.description":
    "Upload a .zip file (must contain memories_history.json) or a .json file.",
  "downloader.workflow.button.upload": "Upload",
  "downloader.workflow.button.remove": "Remove",
  "downloader.workflow.button.startDownload": "Start Download",
  "downloader.workflow.button.processFiles": "Process Files",
  "downloader.workflow.progress.download": "Download progress",
  "downloader.workflow.progress.processing": "Processing progress",
  "downloader.workflow.progress.downloadDetails":
    "{successful}/{total} files downloaded ({status})",
  "downloader.workflow.progress.processDetails":
    "{completed}/{total} files processed (ok: {successful}, failed: {failed})",
  "downloader.workflow.status.idle": "Upload a Snapchat export (.zip or .json) to begin.",
  "downloader.workflow.status.noFileSelected": "No file selected.",
  "downloader.workflow.status.loadingJobState": "Could not load job state.",
  "downloader.workflow.status.unsupportedFile":
    "Unsupported file type. Please choose a .zip or .json file.",
  "downloader.workflow.status.validating": "Validating {fileName}...",
  "downloader.workflow.status.valid": "{fileName} is valid.",
  "downloader.workflow.status.importing": "{fileName} is valid. Importing...",
  "downloader.workflow.status.imported":
    "Imported {importedCount} items. Skipped {skippedDuplicates} duplicates.",
  "downloader.workflow.status.downloading": "Downloading queued media...",
  "downloader.workflow.status.downloaded": "Downloaded {count} files.",
  "downloader.workflow.status.processing": "Processing downloaded files...",
  "downloader.workflow.status.processed":
    "Processed {processedCount} files. Failed {failedCount} files.",
  "downloader.workflow.status.downloadStatus.idle": "idle",
  "downloader.workflow.status.downloadStatus.running": "running",
  "downloader.workflow.status.downloadStatus.success": "success",
  "downloader.workflow.status.downloadStatus.error": "error",
  "downloader.workflow.error.generic": "Operation failed. Please try again.",
  "downloader.workflow.error.zipPathRequired":
    "ZIP validation requires a local file path from the Tauri file picker.",
  "downloader.workflow.error.invalidZip":
    "ZIP is invalid or does not include memories_history.json.",
  "downloader.workflow.error.invalidJson":
    "JSON is invalid or does not match Snapchat memories schema.",
  "downloader.workflow.error.download.EXPIRED_LINK": "The download link expired.",
  "downloader.workflow.error.download.HTTP_ERROR": "A network error occurred while downloading.",
  "downloader.workflow.error.download.IO_ERROR": "A local file write error occurred during download.",
  "downloader.workflow.error.download.CONCURRENCY_ERROR":
    "Download worker synchronization failed.",
  "downloader.workflow.error.download.INTERNAL_ERROR": "An internal download error occurred.",
  "downloader.workflow.error.process.MISSING_DOWNLOADED_FILE":
    "A downloaded source file is missing and cannot be processed.",
  "downloader.workflow.error.process.PROCESSING_FAILED":
    "Media processing failed for one or more files.",

  "viewer.card.title": "Viewer",
  "viewer.card.description": "Browse generated thumbnails in a virtualized grid.",
  "viewer.status.loading": "Loading thumbnails...",
  "viewer.status.loaded": "Loaded {count} thumbnails.",
  "viewer.status.empty": "No thumbnails available yet.",
  "viewer.status.loadFailed": "Could not load thumbnails.",
  "viewer.grid.thumbnailAlt": "Thumbnail {id}",
  "viewer.grid.openMedia": "Open media {id}",
  "viewer.modal.title": "Media viewer",
  "viewer.modal.close": "Close viewer",
  "viewer.modal.soundEnable": "Enable sound",
  "viewer.modal.soundDisable": "Disable sound",
  "viewer.modal.enterFullscreen": "Enter fullscreen",
  "viewer.modal.exitFullscreen": "Exit fullscreen",
  "viewer.modal.rotateLeft": "Rotate left",
  "viewer.modal.rotateRight": "Rotate right",
  "viewer.modal.previous": "Previous media",
  "viewer.modal.next": "Next media",
  "viewer.modal.counter": "{current} / {total}",
  "viewer.modal.imageAlt": "Media {id}",
  "viewer.modal.videoLoading": "Loading video...",
  "viewer.modal.videoUnsupported":
    "Video playback failed in the embedded viewer. On Linux, install system GStreamer codecs (for example: gstreamer1.0-libav, gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-plugins-ugly), then restart the app.",
} as const;

export type TranslationKey = keyof typeof enMessages;
export type TranslationParams = Record<string, string | number>;

const deMessages: Record<TranslationKey, string> = {
  "app.header.title": "MemorySnaper",
  "app.tabs.downloader": "Downloader",
  "app.tabs.viewer": "Betrachter",
  "app.tabs.settings": "Einstellungen",
  "app.header.subtitle": "Phase-1-Tab-Layout-Grundgerüst",
  "app.section.downloader": "Downloader-Bereich",
  "app.section.viewer": "Betrachter-Bereich",
  "app.section.settings": "Einstellungsbereich",

  "settings.card.title": "Einstellungen",
  "settings.card.description": "Sprache, Darstellung und Download-Limits konfigurieren.",
  "settings.form.appearance": "Darstellung",
  "settings.form.theme.light": "Hell",
  "settings.form.theme.system": "System",
  "settings.form.theme.dark": "Dunkel",
  "settings.form.language": "Sprache",
  "settings.form.language.system": "System",
  "settings.form.language.en": "English",
  "settings.form.language.de": "Deutsch",
  "settings.form.language.detected": "Erkannte Sprache: {locale}",
  "settings.form.requestsPerMinute": "Anfragen pro Minute",
  "settings.form.concurrentDownloads": "Gleichzeitige Downloads",
  "settings.form.warning":
    "Warnung: Werte über 100 RPM oder 5 gleichzeitigen Downloads können Drosselung auslösen.",
  "settings.form.reset.button": "Alle App-Daten zurücksetzen",
  "settings.form.reset.inProgress": "Daten werden zurückgesetzt...",
  "settings.form.reset.description":
    "Löscht alle importierten Medieneinträge, Verarbeitungszustände und App-verwaltete Cache-Daten.",
  "settings.form.reset.confirm":
    "Dadurch werden alle App-verwalteten Mediendaten dauerhaft gelöscht und die lokale Datenbank zurückgesetzt. Fortfahren?",
  "settings.form.reset.error": "Zurücksetzen fehlgeschlagen. Bitte erneut versuchen.",
  "settings.form.startupPage": "Startseite",
  "settings.form.startupPage.system": "System (dynamisch)",
  "settings.form.startupPage.downloader": "Downloader",
  "settings.form.startupPage.viewer": "Betrachter",

  "downloader.card.title": "Downloader",
  "downloader.card.description":
    "Snapchat-Exportdaten importieren und ausstehende Erinnerungen herunterladen.",
  "downloader.workflow.upload.title": "Snapchat-Export hochladen",
  "downloader.workflow.upload.description":
    "Lade eine .zip-Datei hoch (muss memories_history.json enthalten) oder eine .json-Datei.",
  "downloader.workflow.button.upload": "Hochladen",
  "downloader.workflow.button.remove": "Entfernen",
  "downloader.workflow.button.startDownload": "Download starten",
  "downloader.workflow.button.processFiles": "Dateien verarbeiten",
  "downloader.workflow.progress.download": "Download-Fortschritt",
  "downloader.workflow.progress.processing": "Verarbeitungsfortschritt",
  "downloader.workflow.progress.downloadDetails":
    "{successful}/{total} Dateien heruntergeladen ({status})",
  "downloader.workflow.progress.processDetails":
    "{completed}/{total} Dateien verarbeitet (ok: {successful}, fehlgeschlagen: {failed})",
  "downloader.workflow.status.idle":
    "Lade einen Snapchat-Export (.zip oder .json) hoch, um zu starten.",
  "downloader.workflow.status.noFileSelected": "Keine Datei ausgewählt.",
  "downloader.workflow.status.loadingJobState": "Auftragsstatus konnte nicht geladen werden.",
  "downloader.workflow.status.unsupportedFile":
    "Nicht unterstützter Dateityp. Bitte wähle eine .zip- oder .json-Datei.",
  "downloader.workflow.status.validating": "{fileName} wird validiert...",
  "downloader.workflow.status.valid": "{fileName} ist gültig.",
  "downloader.workflow.status.importing": "{fileName} ist gültig. Import läuft...",
  "downloader.workflow.status.imported":
    "{importedCount} Elemente importiert. {skippedDuplicates} Duplikate übersprungen.",
  "downloader.workflow.status.downloading": "Ausstehende Medien werden heruntergeladen...",
  "downloader.workflow.status.downloaded": "{count} Dateien heruntergeladen.",
  "downloader.workflow.status.processing": "Heruntergeladene Dateien werden verarbeitet...",
  "downloader.workflow.status.processed":
    "{processedCount} Dateien verarbeitet. {failedCount} Dateien fehlgeschlagen.",
  "downloader.workflow.status.downloadStatus.idle": "inaktiv",
  "downloader.workflow.status.downloadStatus.running": "läuft",
  "downloader.workflow.status.downloadStatus.success": "erfolgreich",
  "downloader.workflow.status.downloadStatus.error": "Fehler",
  "downloader.workflow.error.generic": "Vorgang fehlgeschlagen. Bitte versuche es erneut.",
  "downloader.workflow.error.zipPathRequired":
    "ZIP-Validierung benötigt einen lokalen Dateipfad aus dem Tauri-Dateidialog.",
  "downloader.workflow.error.invalidZip":
    "ZIP ist ungültig oder enthält keine memories_history.json.",
  "downloader.workflow.error.invalidJson":
    "JSON ist ungültig oder entspricht nicht dem Snapchat-Memories-Schema.",
  "downloader.workflow.error.download.EXPIRED_LINK": "Der Download-Link ist abgelaufen.",
  "downloader.workflow.error.download.HTTP_ERROR":
    "Beim Herunterladen ist ein Netzwerkfehler aufgetreten.",
  "downloader.workflow.error.download.IO_ERROR":
    "Beim Download ist ein lokaler Schreibfehler aufgetreten.",
  "downloader.workflow.error.download.CONCURRENCY_ERROR":
    "Die Synchronisierung der Download-Worker ist fehlgeschlagen.",
  "downloader.workflow.error.download.INTERNAL_ERROR":
    "Ein interner Download-Fehler ist aufgetreten.",
  "downloader.workflow.error.process.MISSING_DOWNLOADED_FILE":
    "Eine heruntergeladene Quelldatei fehlt und kann nicht verarbeitet werden.",
  "downloader.workflow.error.process.PROCESSING_FAILED":
    "Die Medienverarbeitung ist für eine oder mehrere Dateien fehlgeschlagen.",

  "viewer.card.title": "Betrachter",
  "viewer.card.description": "Erstellte Vorschaubilder in einem virtualisierten Raster durchsuchen.",
  "viewer.status.loading": "Lade Vorschaubilder...",
  "viewer.status.loaded": "{count} Vorschaubilder geladen.",
  "viewer.status.empty": "Noch keine Vorschaubilder verfügbar.",
  "viewer.status.loadFailed": "Vorschaubilder konnten nicht geladen werden.",
  "viewer.grid.thumbnailAlt": "Vorschaubild {id}",
  "viewer.grid.openMedia": "Medium {id} öffnen",
  "viewer.modal.title": "Medienanzeige",
  "viewer.modal.close": "Anzeige schließen",
  "viewer.modal.soundEnable": "Ton aktivieren",
  "viewer.modal.soundDisable": "Ton deaktivieren",
  "viewer.modal.enterFullscreen": "Vollbild aktivieren",
  "viewer.modal.exitFullscreen": "Vollbild beenden",
  "viewer.modal.rotateLeft": "Nach links drehen",
  "viewer.modal.rotateRight": "Nach rechts drehen",
  "viewer.modal.previous": "Vorheriges Medium",
  "viewer.modal.next": "Nächstes Medium",
  "viewer.modal.counter": "{current} / {total}",
  "viewer.modal.imageAlt": "Medium {id}",
  "viewer.modal.videoLoading": "Video wird geladen...",
  "viewer.modal.videoUnsupported":
    "Die Videowiedergabe in der eingebetteten Anzeige ist fehlgeschlagen. Unter Linux bitte die systemweiten GStreamer-Codecs installieren (z. B. gstreamer1.0-libav, gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-plugins-ugly) und die App neu starten.",
};

type TranslationDictionary = Record<TranslationKey, string>;

export const messagesByLocale: Record<ResolvedLocale, TranslationDictionary> = {
  en: enMessages,
  de: deMessages,
};