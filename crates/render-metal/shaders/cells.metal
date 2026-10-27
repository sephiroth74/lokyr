// The shaders of a frame: rectangles, the backgrounds of a grid's cells, and
// glyphs, of coverage or of their own colors, read from the atlas a pixel a
// pixel. Every shape is an instance of a quad, whose four corners come from
// the vertex's index, drawn as a strip.
//
// Colors are sRGB's encoded components, premultiplied by their opacity, and
// the drawable's format keeps them encoded: blending mixes them as they are,
// as macOS blends text. A drawable that is not opaque lets what is behind
// the window show through.

#include <metal_stdlib>
using namespace metal;

// The drawable's size, in pixels.
struct Viewport {
    float2 size;
};

// A grid's top left corner and a cell's size, in pixels, and its columns.
struct Grid {
    float2 origin;
    float2 cell;
    uint columns;
};

// A rectangle of one color.
struct Rect {
    float2 position;
    float2 size;
    float4 color;
};

// A glyph: where it goes and its size, in pixels; where its pixels are in
// the atlas; the color that fills a glyph of coverage.
struct Glyph {
    float2 position;
    float2 size;
    uint2 atlas;
    float4 color;
};

struct Fragment {
    float4 position [[position]];
    float4 color [[flat]];
    uint2 atlas [[flat]];
    // The pixel within the glyph, at its centre.
    float2 local;
};

// The corner of a quad's strip: 0 top left, 1 top right, 2 bottom left, 3
// bottom right.
static float2 corner(uint vertex_id) {
    return float2(vertex_id & 1, vertex_id >> 1);
}

static float4 clip_space(float2 pixel, float2 viewport) {
    return float4(pixel.x / viewport.x * 2.0 - 1.0, 1.0 - pixel.y / viewport.y * 2.0, 0.0, 1.0);
}

vertex Fragment rect_vertex(uint vertex_id [[vertex_id]],
                            uint instance [[instance_id]],
                            constant Rect *rects [[buffer(0)]],
                            constant Viewport &viewport [[buffer(1)]]) {
    Rect rect = rects[instance];
    Fragment out;
    out.position = clip_space(rect.position + corner(vertex_id) * rect.size, viewport.size);
    out.color = rect.color;
    out.atlas = uint2(0);
    out.local = float2(0);
    return out;
}

vertex Fragment background_vertex(uint vertex_id [[vertex_id]],
                                  uint instance [[instance_id]],
                                  constant float4 *colors [[buffer(0)]],
                                  constant Viewport &viewport [[buffer(1)]],
                                  constant Grid &grid [[buffer(2)]]) {
    float2 cell = float2(instance % grid.columns, instance / grid.columns);
    Fragment out;
    out.position = clip_space(grid.origin + (cell + corner(vertex_id)) * grid.cell, viewport.size);
    out.color = colors[instance];
    out.atlas = uint2(0);
    out.local = float2(0);
    return out;
}

fragment float4 solid_fragment(Fragment in [[stage_in]]) {
    return in.color;
}

vertex Fragment glyph_vertex(uint vertex_id [[vertex_id]],
                             uint instance [[instance_id]],
                             constant Glyph *glyphs [[buffer(0)]],
                             constant Viewport &viewport [[buffer(1)]]) {
    Glyph glyph = glyphs[instance];
    float2 local = corner(vertex_id) * glyph.size;
    Fragment out;
    out.position = clip_space(glyph.position + local, viewport.size);
    out.color = glyph.color;
    out.atlas = glyph.atlas;
    out.local = local;
    return out;
}

// A glyph of coverage, filled with its color, which comes premultiplied.
fragment float4 coverage_fragment(Fragment in [[stage_in]],
                                  texture2d<float, access::read> atlas [[texture(0)]]) {
    float coverage = atlas.read(in.atlas + uint2(in.local)).r;
    return in.color * coverage;
}

// A glyph of its own colors, premultiplied as the atlas holds it.
fragment float4 color_fragment(Fragment in [[stage_in]],
                               texture2d<float, access::read> atlas [[texture(0)]]) {
    return atlas.read(in.atlas + uint2(in.local));
}
