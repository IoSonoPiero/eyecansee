// EyeCanSee's CAD engine: a small C interface over Open CASCADE Technology (LGPL 2.1 with the
// OCCT exception), built by tools/build-occt-linux.sh into one library downloaded when needed
// (src/component.rs). It reads a STEP, IGES or BREP file and gives its surfaces as triangles,
// placed as in the assembly, with the colour of each triangle. Read by src/occt.rs.

#include <BRepBndLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <IGESCAFControl_Reader.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <RWMesh_FaceIterator.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Version.hxx>
#include <TDocStd_Document.hxx>
#include <TopoDS_Compound.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFPrs_DocumentExplorer.hxx>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <string>
#include <vector>

#ifdef _WIN32
#define ECS_EXPORT extern "C" __declspec(dllexport)
#else
#define ECS_EXPORT extern "C" __attribute__((visibility("default")))
#endif

extern "C" {
// The interface's version: src/occt.rs refuses a library with another one.
#define ECS_OCCT_API 1

// Filled by ecs_occt_read; freed with ecs_occt_free.
typedef struct EcsMesh {
    float* pos;        // 3 per vertex
    uint32_t vertices;
    uint32_t* tris;    // 3 per triangle, indices into pos
    uint32_t triangles;
    uint8_t* colors;   // 3 per triangle (sRGB), or NULL when the file has none
    char error[512];   // set when ecs_occt_read fails
} EcsMesh;
}

enum Kind { STEP = 0, IGES = 1, BREP = 2 };

static void fail(EcsMesh* out, const std::string& what) {
    std::strncpy(out->error, what.c_str(), sizeof(out->error) - 1);
    out->error[sizeof(out->error) - 1] = 0;
}

// The file into an XCAF document: shapes, assembly placements and colours.
static Handle(TDocStd_Document) load(const char* path, int kind, EcsMesh* out) {
    Handle(TDocStd_Document) doc;
    XCAFApp_Application::GetApplication()->NewDocument("BinXCAF", doc);
    if (kind == STEP) {
        STEPCAFControl_Reader reader;
        reader.SetColorMode(true);
        reader.SetNameMode(false);
        reader.SetLayerMode(false);
        if (reader.ReadFile(path) != IFSelect_RetDone) {
            fail(out, "not a STEP file, or damaged");
            return nullptr;
        }
        if (!reader.Transfer(doc)) {
            fail(out, "nothing could be read from the STEP file");
            return nullptr;
        }
    } else if (kind == IGES) {
        IGESCAFControl_Reader reader;
        reader.SetColorMode(true);
        reader.SetNameMode(false);
        reader.SetLayerMode(false);
        if (reader.ReadFile(path) != IFSelect_RetDone) {
            fail(out, "not an IGES file, or damaged");
            return nullptr;
        }
        if (!reader.Transfer(doc)) {
            fail(out, "nothing could be read from the IGES file");
            return nullptr;
        }
    } else {
        TopoDS_Shape shape;
        BRep_Builder builder;
        if (!BRepTools::Read(shape, path, builder) || shape.IsNull()) {
            fail(out, "not a BREP file, or damaged");
            return nullptr;
        }
        XCAFDoc_DocumentTool::ShapeTool(doc->Main())->AddShape(shape);
    }
    return doc;
}

// Triangles for every surface, fine enough for the size of the whole model.
static bool mesh(const Handle(TDocStd_Document)& doc, EcsMesh* out) {
    NCollection_Sequence<TDF_Label> roots;
    XCAFDoc_DocumentTool::ShapeTool(doc->Main())->GetFreeShapes(roots);
    TopoDS_Compound all;
    BRep_Builder builder;
    builder.MakeCompound(all);
    for (const TDF_Label& label : roots) {
        TopoDS_Shape shape = XCAFDoc_ShapeTool::GetShape(label);
        if (!shape.IsNull()) builder.Add(all, shape);
    }
    Bnd_Box box;
    BRepBndLib::Add(all, box);
    if (box.IsVoid()) {
        fail(out, "no surfaces in the file");
        return false;
    }
    const double diag = std::sqrt(box.SquareExtent());
    // About 1/2000 of the model: smooth on screen, not too many triangles.
    BRepMesh_IncrementalMesh(all, diag * 5e-4, false, 0.35, true);
    return true;
}

ECS_EXPORT int ecs_occt_api(void) {
    return ECS_OCCT_API;
}

ECS_EXPORT const char* ecs_occt_version(void) {
    return OCC_VERSION_COMPLETE;
}

// kind: 0 STEP, 1 IGES, 2 BREP. Returns 0 when read, otherwise `out->error` says why.
ECS_EXPORT int ecs_occt_read(const char* path, int kind, EcsMesh* out) {
    std::memset(out, 0, sizeof(*out));
    try {
        // No messages on the terminal of the app.
        Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_Printer));
        Handle(TDocStd_Document) doc = load(path, kind, out);
        if (doc.IsNull() || !mesh(doc, out)) return 1;
        std::vector<float> pos;
        std::vector<uint32_t> tris;
        std::vector<uint8_t> colors;
        bool coloured = false;
        for (XCAFPrs_DocumentExplorer parts(doc, XCAFPrs_DocumentExplorerFlags_OnlyLeafNodes); parts.More(); parts.Next()) {
            const XCAFPrs_DocumentNode& node = parts.Current();
            for (RWMesh_FaceIterator face(node.RefLabel, node.Location, true, node.Style); face.More(); face.Next()) {
                if (face.IsEmpty()) continue;
                const size_t base = pos.size() / 3;
                if (base + face.NbNodes() > UINT32_MAX / 2) {
                    fail(out, "too many triangles");
                    return 1;
                }
                for (int n = face.NodeLower(); n <= face.NodeUpper(); ++n) {
                    gp_Pnt p = face.NodeTransformed(n);
                    pos.push_back((float)p.X());
                    pos.push_back((float)p.Y());
                    pos.push_back((float)p.Z());
                }
                uint8_t rgb[3] = {200, 205, 214};
                if (face.HasFaceColor()) {
                    coloured = true;
                    double r, g, b;
                    face.FaceColor().GetRGB().Values(r, g, b, Quantity_TOC_sRGB);
                    rgb[0] = (uint8_t)std::lround(r * 255.0);
                    rgb[1] = (uint8_t)std::lround(g * 255.0);
                    rgb[2] = (uint8_t)std::lround(b * 255.0);
                }
                for (int t = face.ElemLower(); t <= face.ElemUpper(); ++t) {
                    Poly_Triangle tri = face.TriangleOriented(t);
                    for (int k = 1; k <= 3; ++k) tris.push_back((uint32_t)(base + tri.Value(k) - face.NodeLower()));
                    colors.insert(colors.end(), rgb, rgb + 3);
                }
            }
        }
        if (tris.empty()) {
            fail(out, "no surfaces in the file");
            return 1;
        }
        out->vertices = (uint32_t)(pos.size() / 3);
        out->triangles = (uint32_t)(tris.size() / 3);
        out->pos = (float*)std::malloc(pos.size() * sizeof(float));
        out->tris = (uint32_t*)std::malloc(tris.size() * sizeof(uint32_t));
        if (!out->pos || !out->tris) {
            fail(out, "out of memory");
            return 1;
        }
        std::memcpy(out->pos, pos.data(), pos.size() * sizeof(float));
        std::memcpy(out->tris, tris.data(), tris.size() * sizeof(uint32_t));
        if (coloured) {
            out->colors = (uint8_t*)std::malloc(colors.size());
            if (out->colors) std::memcpy(out->colors, colors.data(), colors.size());
        }
        return 0;
    } catch (const Standard_Failure& e) {
        fail(out, std::string("CAD engine: ") + e.what());
    } catch (const std::exception& e) {
        fail(out, std::string("CAD engine: ") + e.what());
    } catch (...) {
        fail(out, "CAD engine: unknown error");
    }
    return 1;
}

ECS_EXPORT void ecs_occt_free(EcsMesh* m) {
    std::free(m->pos);
    std::free(m->tris);
    std::free(m->colors);
    m->pos = nullptr;
    m->tris = nullptr;
    m->colors = nullptr;
}
