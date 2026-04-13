
### Stuff need to Fix:


make the missing file donwlaod section load faster, currently it takes quite long so the list will load


thumbnails are not reloaded in the viewer since update to be more consistent with the new data structure. here need update the viewer to reload the thumbnail when the session is loaded, here it needs trigger to reload the cahched data of the viewer when a session is started or stopped from downloader/extractor



when i restart the app, and a session will automaticly start in run state



mark files where overlay failed in db
general seems that all videos are missing the overlay. here analyze the codebase and the files. why this happens?



data reset failed after running the extractor
	not fully resset, logs, status. also seems that depsite stopped a mp4 was still running in bg, which maybe failed the reset data through settings. here also fully shutdown the downloader/extractor scripts




update logs. use timestamp for date, eg [10:19:40]. than state if its a improted img or video and also maybe its timestamp and translated location.

[2019-05-16] Extracting mid 0c999aa2 from ZIP archives (3301/5086)

[2019-04-18] Extracting mid ed53d0d8 from ZIP archives (3302/5086)






add spacing between side bar menu items + confirm window settings reset data



Hardware acceleration — On Windows, could detect NVENC (h264_nvenc) or QSV for much faster encoding. Significant optimization but adds codec detection complexity. Recommend deferring to a follow-up.
Downscale-with-sharpen alternative — When resolution gap is extreme (>2×), upscaling the video is heavy. Could instead downscale overlay with a sharpening filter to preserve text. Recommend defaulting to upscale but adding as a config option later.


