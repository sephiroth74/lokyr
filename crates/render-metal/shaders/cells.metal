// The shaders of a frame: rectangles, the backgrounds of a grid's cells,
// glyphs, of coverage or of their own colors, read from the atlas a pixel a
// pixel, the lines that decorate text, of their patterns, and the
// interface's shapes, of rounded corners, borders and shadows. Every shape is an instance of a quad, whose four corners come from
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
// the atlas; which atlas, 0 for coverage's and 1 for colors'; the color
// that fills a glyph of coverage.
struct Glyph {
    float2 position;
    float2 size;
    uint2 atlas;
    uint texture;
    uint padding;
    float4 color;
};

struct Fragment {
    float4 position [[position]];
    float4 color [[flat]];
    uint2 atlas [[flat]];
    uint texture [[flat]];
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
    out.texture = 0;
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
    out.texture = 0;
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
    out.texture = glyph.texture;
    out.local = local;
    return out;
}

// A glyph of coverage, filled with its color, which comes premultiplied; or
// of its own colors, premultiplied as the atlas holds them. The two kinds
// draw in one pass, each over those before it.
fragment float4 glyph_fragment(Fragment in [[stage_in]],
                               texture2d<float, access::read> coverage [[texture(0)]],
                               texture2d<float, access::read> colors [[texture(1)]]) {
    uint2 pixel = in.atlas + uint2(in.local);
    if (in.texture == 1) {
        return colors.read(pixel);
    }
    return in.color * coverage.read(pixel).r;
}

// A line across cells: the band of pixels that its pattern takes; its
// color; how it is drawn, as `LineStyle` numbers it from 0, single, double,
// curly, dotted and dashed; the thickness of its strokes; and the period of
// a wave and of dashes, a cell's width, from `origin`, the grid's left edge,
// so that a pattern goes on across the cells and the lines of a row.
struct Line {
    float2 position;
    float2 size;
    float4 color;
    uint style;
    float thickness;
    float period;
    float origin;
};

struct LineFragment {
    float4 position [[position]];
    float4 color [[flat]];
    uint style [[flat]];
    float thickness [[flat]];
    float period [[flat]];
    float origin [[flat]];
    float2 size [[flat]];
    // The pixel within the band, at its centre.
    float2 local;
};

vertex LineFragment line_vertex(uint vertex_id [[vertex_id]],
                                uint instance [[instance_id]],
                                constant Line *lines [[buffer(0)]],
                                constant Viewport &viewport [[buffer(1)]]) {
    Line line = lines[instance];
    float2 local = corner(vertex_id) * line.size;
    LineFragment out;
    out.position = clip_space(line.position + local, viewport.size);
    out.color = line.color;
    out.style = line.style;
    out.thickness = line.thickness;
    out.period = line.period;
    out.origin = line.origin;
    out.size = line.size;
    out.local = local;
    return out;
}

// How much of a pixel a line's pattern covers.
static float line_coverage(LineFragment in) {
    float t = max(in.thickness, 1.0);
    // The pixel's column from the grid's left edge, whole.
    float x = floor(in.position.x - in.origin);
    switch (in.style) {
        // Double: a stroke at the band's top and one at its bottom.
        case 1:
            return (in.local.y < t || in.local.y >= in.size.y - t) ? 1.0 : 0.0;
        // Curly: a wave a cell long, of the stroke's thickness, smoothed.
        case 2: {
            float amplitude = max((in.size.y - t) / 2.0, 0.0);
            float phase = 2.0 * M_PI_F * (in.position.x - in.origin) / in.period;
            float middle = in.size.y / 2.0 - amplitude * sin(phase);
            float slope = amplitude * 2.0 * M_PI_F / in.period * cos(phase);
            float distance = abs(in.local.y - middle) / sqrt(1.0 + slope * slope);
            return clamp(t / 2.0 + 0.5 - distance, 0.0, 1.0);
        }
        // Dotted: dots of the thickness, as far apart.
        case 3:
            return fmod(floor(x / t), 2.0) == 0.0 ? 1.0 : 0.0;
        // Dashed: a dash in the middle of each cell, of three fifths of it.
        case 4: {
            float within = fmod(x, in.period);
            float dash = round(in.period * 0.6);
            float start = floor((in.period - dash) / 2.0);
            return (within >= start && within < start + dash) ? 1.0 : 0.0;
        }
        default:
            return 1.0;
    }
}

fragment float4 line_fragment(LineFragment in [[stage_in]]) {
    return in.color * line_coverage(in);
}

// A shape of the interface: the quad that it covers; its outline's centre,
// half its size, and its corners' radius; its border's width, inside its
// edges; the deviation of its blur, for a shadow, or a negative one for the
// shape itself; what fills it, or its shadow; and its border's color.
struct Shape {
    float2 position;
    float2 size;
    float2 center;
    float2 extent;
    float radius;
    float border;
    float sigma;
    float padding;
    float4 fill;
    float4 border_color;
};

struct ShapeFragment {
    float4 position [[position]];
    float2 center [[flat]];
    float2 extent [[flat]];
    float radius [[flat]];
    float border [[flat]];
    float sigma [[flat]];
    float4 fill [[flat]];
    float4 border_color [[flat]];
};

vertex ShapeFragment shape_vertex(uint vertex_id [[vertex_id]],
                                  uint instance [[instance_id]],
                                  constant Shape *shapes [[buffer(0)]],
                                  constant Viewport &viewport [[buffer(1)]]) {
    Shape shape = shapes[instance];
    ShapeFragment out;
    out.position = clip_space(shape.position + corner(vertex_id) * shape.size, viewport.size);
    out.center = shape.center;
    out.extent = shape.extent;
    out.radius = min(shape.radius, min(shape.extent.x, shape.extent.y));
    out.border = shape.border;
    out.sigma = shape.sigma;
    out.fill = shape.fill;
    out.border_color = shape.border_color;
    return out;
}

// The distance from `p` to the outline of a rectangle of rounded corners
// about the origin, of half its size `extent`: negative inside.
static float rounded_box(float2 p, float2 extent, float radius) {
    float2 q = abs(p) - extent + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

// The error function, within 5e-4: Abramowitz and Stegun's 7.1.27.
static float erf_approximation(float x) {
    float a = abs(x);
    float t = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
    t *= t;
    return sign(x) * (1.0 - 1.0 / (t * t));
}

// A shadow covers a pixel as a Gaussian blur of the outline's edge, the
// error function across its distance; a shape, the part of the pixel within
// its outline, filled within its border's inner outline.
fragment float4 shape_fragment(ShapeFragment in [[stage_in]]) {
    float2 p = in.position.xy - in.center;
    float distance = rounded_box(p, in.extent, in.radius);
    if (in.sigma >= 0.0) {
        float cover = in.sigma > 0.0
            ? 0.5 - 0.5 * erf_approximation(distance / (in.sigma * M_SQRT2_F))
            : clamp(0.5 - distance, 0.0, 1.0);
        return in.fill * cover;
    }
    float outer = clamp(0.5 - distance, 0.0, 1.0);
    float inner = outer;
    if (in.border > 0.0) {
        float2 extent = in.extent - in.border;
        inner = 0.0;
        if (extent.x > 0.0 && extent.y > 0.0) {
            float within = rounded_box(p, extent, max(in.radius - in.border, 0.0));
            inner = min(clamp(0.5 - within, 0.0, 1.0), outer);
        }
    }
    return in.fill * inner + in.border_color * (outer - inner);
}
