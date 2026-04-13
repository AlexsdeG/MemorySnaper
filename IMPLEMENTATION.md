
### Stuff need to Fix:


make the missing file donwlaod section load faster, currently it takes quite long so the list will load


thumbnails are not reloaded in the viewer since update to be more consistent with the new data structure. here need update the viewer to reload the thumbnail when the session is loaded, here it needs trigger to reload the cahched data of the viewer when a session is started or stopped from downloader/extractor



when i restart the app, and a session will automaticly start in run state



mark files where overlay failed in db
general seems that all videos are missing the overlay. here analyze the codebase and the files. why this happens?



data reset failed after running the extractor
	not fully resset, logs, status. also seems that depsite stopped a mp4 was still running in bg, which maybe failed the reset data through settings. here also fully shutdown the downloader/extractor scripts




pelase update the logs on the downlaoder/extractor page. use timestamp for current time, eg [10:19:40]. than state if its a import img or video and also dispaly its date, format (vid/img) and mid (id of the memory). here update all logs to be better for user to dispaly information. also maybe colored eg for error red; or issues/skip, missing yellow




next please help and add spacing between side bar menu items. here add some gap between the viewer, extractor and settings sidebar items. also add some gap between the confirm window settings reset data reset and cancel button.

