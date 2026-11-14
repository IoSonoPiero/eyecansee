# Exporting 3D models

EyeCanSee can save a 3D model in another 3D format, for example to open a model from a CAD program in a 3D printing program, or to send a model in a format that more programs read.

## How to export

1. Select one or more 3D models in the grid (or open one in the viewer).
2. Right-click and choose **Export as a 3D model…**, or use **File > Export as a 3D model…**.
3. In the window, choose the **format**: **STL**, **OBJ**, **PLY** or **glTF (GLB)** (the last one used is proposed), then click **Export**.

Each new file is saved next to its original, with the same name and the new extension. If that name is already taken, " (2)" is added, for example `bracket (2).stl`. The status bar says the name it got; if something goes wrong it says why.

With other kinds of files selected too, the entry is marked with **\*** and works only on the models. CAD drawings are exported the same way, with **File > Export drawing…**: see [CAD drawings](cad.md).

## What each format keeps

EyeCanSee keeps as much of the original as the new format can hold:

- **glTF (GLB)** keeps the pictures on the surfaces (textures), the normals and the materials (how smooth or metallic they are, bumps, shadows in the creases, their own light).
- **OBJ** keeps the normals and the colours.
- **PLY** and **STL** keep a colour for each triangle.

Where a format cannot hold a picture, its colours are taken from the picture, so the model does not come out grey.

## Which way is up

Each file is written with the up axis other programs expect: Z for STL and PLY, Y for OBJ and glTF. So the model opens standing in those programs too. For this reason the axes in the corner of the viewer can differ from those of the original.

## Good to know

- To save a model as a picture instead, see [3D models](model-3d.md) (Ctrl+Shift+S in the viewer).
- The models EyeCanSee opens are listed in [Supported formats](formats.md).
