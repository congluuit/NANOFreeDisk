# The shared selection bar of every list the user ticks before acting
# (SelectionBar.svelte, ConfirmTotals.svelte, GroupCheck.svelte). Every key must
# exist in vi/selection.ftl too.

sel-summary = { $count } selected · { $files } files · { $size } will be freed
sel-summary-nofiles = { $count } selected · { $size } will be freed
sel-none = Nothing selected yet — tick items, or use the buttons below.
sel-skip = { $count } item(s) may be skipped because { $apps } is running ({ $size } not counted)
sel-group-label = Selection
sel-all = Select all
sel-clear = Clear selection
sel-only = Only "{ $level }"
sel-only-hint = Replace the selection with every item of this risk level.
sel-keys-hint = Ctrl+A selects everything in this list, Esc clears the selection.
sel-group = Select everything in { $name }
sel-clean-n = Clean { $count } item(s) ({ $size })
sel-recycle-n = Move { $count } to the Recycle Bin ({ $size })
sel-quarantine-n = Move { $count } to the quarantine ({ $size })
sel-confirm-total = { $count } item(s) · { $size } will be freed
sel-confirm-level = { $count } item(s) · { $size }
sel-ack = I understand: the Caution / Advanced items above lose history or settings, or must be downloaded again.
