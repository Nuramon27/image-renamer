Griphook is a little tool to batch-rename a set of images.
It lets you view images in a folder, select some of them for renaming,
and transform their name by matching a regular expression.

You open it with a `--dir` argument containing directory in which you want to rename
image files. If left out, Griphook will work on the current directory. It shows you a list
of all image files in the selected directory. You can click on an image to view it
in the image view area, and you can select one or more image files for renaming.

On the top there are three text fields: The top left one contains a regular expression with
several capture groups. The top right contains a replacement string where these
capture groups appear as `$group`. When renaming, Griphook will match the regular expression
to every selected file and rename them to the replacement string, expanding these captured
groups to the captured values. The special variable `$name` will obtain the value
entered in the lower text field.

== Selecting files

If you want to preview a file, you simply left-click onto its name.

For selecting files for renaming, there are multiple ways: First you can use the button
right to every file to select or deselect it. The same is done by holding `ctrl` and
left-clicking.

You can also select multiple files at once by holding `ctrl` + `shift` and then clicking on
a file. This will select all files between the one that is currently displayed in the image
preview area, and the one you clicked on.