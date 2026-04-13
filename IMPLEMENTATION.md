
### Stuff need to Fix:


next fix a small issue. here make the missing file donwlaod section in downloader/extraktor load faster, currently it takes quite long that the list will load


when i restart the app the last session will start in run state. here the button are all in state like its running and status also showing running. here i checked nothing is actually running, no process or funciton. here it seems only visual that the state is wrong after restarting the app. here check and fix this.






please help with my tauri app i develop. here i need help with my thumbnails, they  are not reloaded in the viewer since update to be more consistent with the new data structure. here need update the viewer to reload the thumbnail when the session is loaded, here it needs trigger to reload the cahched data of the viewer when a session is started or stopped from downloader/extractor. here the issue is that when i load new data into the viewer it shows the old thumbnails for the grid items. here this could be cause i updated the viewer to preload stuff into the ram to be lighning fast when using on a device cause i have large amount of thumbnailsto show.



next please fix a small bug when i reset data while running the extractor, here it fails and shows error msg.
	also sometimes it not fully reset, logs, status. also seems that depsite stopped a mp4 was still running in bg, which maybe failed the reset data through settings. here also fully shutdown the downloader/extractor scripts function when reset data


