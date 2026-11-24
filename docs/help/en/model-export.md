# Exporting 3D models

EyeCanSee can save a 3D model in another 3D format, for example to open a model from a CAD program in a 3D printing program, or to send a model in a format that more programs read.

## How to export

1. Select one or more 3D models in the grid (or open one in the viewer).
2. Right-click and choose **Export as a 3D model…**, or use **File > Export as a 3D model…**.
3. In the window, choose the **format**: **STL**, **OBJ**, **PLY** or **glTF (GLB)** (the last one used is proposed), then click **Export**. Enter does the same; Esc or **Cancel** closes the window without exporting.

Each new file is saved next to its original, with the same name and the new extension. If that name is already taken, " (2)" is added, for example `bracket (2).stl`. The status bar says the name it got.

While the models are exported, a box at the bottom right shows the model being exported, which one it is of how many, and how far the export is. **Cancel** in the box (also reached with **F6**) stops it: the models already exported stay, the file being written is removed, and the others are not exported. A second export started meanwhile waits for the first and joins its box. If a copy or a conversion ends close to it, they say it in one report, **Jobs finished**, where an export with nothing to say has its line (**Export finished** with how many models, or **Export cancelled** with how many were exported before **Cancel**). If a model cannot be exported, or the new file is not quite like the original (a glTF that places one of its parts more than once in its hierarchy, which the format does not allow: the new file has it once), a message says so when the export is done, **Export finished**: one message for all the models, which says how many were not exported and how many with a change, with their names in order (the first ten, then how many more) and, next to each model not exported, why in a few words when it is known. ![Copy the message](icon://copy) copies it with every name and the whole reason.

EyeCanSee opens OBJ files of up to 10 million triangles, binary STL and GLB files of up to 20 million and binary PLY files of up to 50 million (see [3D models](model-3d.md)). A model with more triangles than the format chosen allows can still be exported, but EyeCanSee will not be able to open the new file: for a model already read in the viewer the window says so under the format, and the **Export finished** message lists the others, each with the most triangles its new format allows.

With other kinds of files selected too, the entry is marked with **\*** and works only on the models. CAD drawings are exported the same way, with **File > Export drawing…**: see [CAD drawings](cad.md).

## What each format keeps

EyeCanSee keeps as much of the original as the new format can hold:

- **glTF (GLB)** keeps the pictures on the surfaces (textures), the normals and the materials (how smooth or metallic they are, bumps, shadows in the creases, their own light).
- **OBJ** keeps the pictures on the surfaces, their coordinates, the normals, the colours and the materials (how smooth or metallic they are, bumps, their own light). The pictures are saved as PNG files next to the model, and the materials in a `.mtl` file with the model's name (`bracket.mtl`, `bracket.png`, or `bracket-1.png`, `bracket-2.png`… with more pictures): keep them in the same folder as the OBJ. Their names have no spaces, which many programs do not read in these files: a space in the model's name becomes "_" (`my bracket.obj` gets `my_bracket.mtl`). A name already taken there gets "_2" (then "_3"…), so no file is written over. The triangles that meet share their points, so other programs see one surface.
- **PLY** keeps the normals and the colours, both on the points (which most programs read) and on each triangle.
- **STL** keeps a colour for each triangle.

Where a format cannot hold a picture, its colours are taken from the picture, so the model does not come out grey. What the new format cannot hold is said in the **Export finished** message: the models exported without their pictures (STL and PLY), and those exported without all of their materials (STL and PLY have no place for them; OBJ none for the pictures of roughness, metal and shadows in the creases). glTF (GLB) keeps them all, as EyeCanSee read them: pictures larger than 2048 pixels are read, and saved, at 2048, and of a model with more than 64 pictures only the first 64 are kept; the **Export finished** message says so for each model.

Names of the parts, groups and their hierarchy, and transparency are not kept in any format: EyeCanSee reads a model as one set of triangles.

## Which way is up

Each file is written with the up axis other programs expect: Z for STL and PLY, Y for OBJ and glTF. So the model opens standing in those programs too. For this reason the axes in the corner of the viewer can differ from those of the original.

## Units

glTF uses metres. A model whose file says its unit (STEP and IGES in millimetres, 3MF and AMF as they say) is converted to metres when it is exported as glTF, so it keeps its size: a part of 100 mm becomes 0.1 m. STL, OBJ and PLY have no place for the unit: the numbers stay as they are, and the unit is written at the top of the file only as a comment, which other programs do not read, and neither does EyeCanSee: opened again, those files have measures without a unit.

## Good to know

- To save a model as a picture instead, see [3D models](model-3d.md) (**Ctrl + Shift + S** in the viewer).
- The models EyeCanSee opens are listed in [Supported formats](formats.md).
