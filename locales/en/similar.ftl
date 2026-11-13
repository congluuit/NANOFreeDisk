# Similar photos in the Duplicates view (docs/SIMILAR_PHOTOS.md). Every key must
# also be in vi/similar.ftl.

similar-mode-label = What to look for
similar-mode-identical = Identical files
similar-mode-identical-hint = Files with exactly the same bytes. One copy of each group always stays.
similar-mode-similar = Similar photos
similar-mode-similar-hint = Photos that look the same but are not the same file: resized, sent through a chat app, exported twice or slightly cropped.

similar-notice-title = These photos look alike, they are not identical copies.
similar-notice-body = Nothing is selected for you. Open each group, compare the photos and tick only the ones you really do not need. The best one of each group is marked.

similar-find-mine = Find similar photos in my files
similar-find-mine-again = Search my files again
similar-find-mine-hint = If you press this: your Documents, Downloads, Pictures, Videos, Music and Desktop folders are read and compared, nothing is changed. Cloud files that are only online are never downloaded.
similar-choose = Choose folders or drives
similar-choose-hint = If you press this: you pick the folders or drives to compare instead of your personal folders.
similar-use-scan = Use the Analyze scan
similar-find = Find similar photos

similar-preset = How alike
similar-preset-strict = Strict
similar-preset-normal = Normal
similar-preset-loose = Loose
similar-preset-strict-hint = Strict: the same photo at another size, compressed again by a chat app, exported twice or with other details. The safest choice.
similar-preset-normal-hint = Normal: also stronger compression, small edits and slight crops. Look at each group before removing anything.
similar-preset-loose-hint = Loose: also visible crops and edits. Photos taken one after another may join a group, so check every photo.

similar-phase-scan = Listing files… { $files } found
similar-phase-collect = Looking for photos…
similar-phase-hash = Reading photos: { $done } / { $total }
similar-phase-compare = Comparing photos: { $done } / { $total }
similar-cancelled = The search for similar photos was cancelled.

similar-headline = { $size } in photos that look like a better one, { $count ->
    [one] in one group
   *[other] in { $count } groups
}
similar-headline-note = Similar is not identical: nothing is selected. Tick the photos you want gone, or use "Keep only the best" in a group.
similar-headline-none = No look-alike photos found.
similar-stats = { $images } photos compared, { $decoded } read now, { $cached } from the cache, { $seconds } s.
similar-none = No look-alike photos with this setting. A looser setting may find more.

similar-not-compared = Not compared: { $count } files
similar-skip-small = Too small
similar-skip-small-why = Pictures under { $size } are icons, stickers and web graphics, not photos.
similar-skip-cloud = Online only
similar-skip-cloud-why = Cloud files that are only online are never opened, so they are never downloaded.
similar-skip-reparse = Links
similar-skip-reparse-why = Shortcuts and links to other places are not followed.
similar-skip-system = System files
similar-skip-system-why = Files Windows marks as system files are left alone.
similar-skip-protected = Protected
similar-skip-protected-why = Places NANOFreeDisk never removes from (system folders, programs, credentials, AI conversation history).
similar-skip-app-dir = In application folders
similar-skip-app-dir-why = Pictures inside programs and packages belong to them. Expert mode can list them, never select them.
similar-skip-unreadable = Could not be read
similar-skip-unreadable-why = Damaged, cut short, locked by another program, or a picture type this version cannot read (HEIC, RAW and TIFF photos are not compared yet).
similar-skip-changed = Changed while reading
similar-skip-changed-why = The file changed while it was read; search again later.
similar-skip-too-large = Too large
similar-skip-too-large-why = Over 64 MB or over 100 megapixels: left out to keep the search light.
similar-skip-plain = No detail
similar-skip-plain-why = A plain colour or a blank page has nothing to compare.

similar-group-title = { $count } look-alike photos
similar-best-pixels = The best one has the most pixels.
similar-best-size = Same size in pixels: the best one is the largest file.
similar-best-place = Same picture and size: the best one is the one outside Downloads, Desktop and Temp.
similar-group-picked = { $count } ticked
similar-extra-hint = Space taken by the photos other than the best one (identical copies are counted in Identical files).
similar-all-but-best = Keep only the best
similar-all-but-best-hint = If you press this: every photo of this group except the best one is ticked for removal. Photos in cloud, git, application or read-only folders stay unticked.
similar-untick = Untick
similar-untick-all = Untick all
similar-best = Best
similar-remove = Remove
similar-tile-tick = If you click this: this photo is ticked for removal.
similar-tile-untick = If you click this: this photo stays.
similar-mp = { $mp } MP
similar-identical = Identical copy
similar-identical-hint = Byte for byte the same file as { $name }.
similar-compare = Compare side by side
similar-compare-title = Compare
similar-close = Close
similar-all-ticked = Every photo of this group is ticked, so none of them would stay.
similar-keep-none = Remove all photos of this group, I do not need any of them

similar-picked = { $size } ticked: { $count ->
    [one] one photo
   *[other] { $count } photos
}
similar-blocked = { $count ->
    [one] One group has every photo ticked: tick "Remove all photos" in it or untick one photo.
   *[other] { $count } groups have every photo ticked: tick "Remove all photos" in them or untick one photo.
}
similar-confirm-recycle = Move these photos to the Recycle Bin?
similar-confirm-quarantine = Move these photos to the quarantine?
similar-confirm-body = { $files } photos from { $groups } groups, { $size }.
similar-confirm-note = These are look-alikes you picked yourself. Each photo is checked again right before the move; anything that changed or became online only is skipped.
similar-confirm-emptied = { $count ->
    [one] One group loses every photo, as you confirmed.
   *[other] { $count } groups lose every photo, as you confirmed.
}

activity-kind-similar = Similar photos removed
reason-not-similar = Not one of the photos you ticked, so it was left alone.
