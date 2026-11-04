// The shaders of a frame: rectangles, the backgrounds of a grid's cells,
// glyphs, of coverage or of their own colors, read from the atlas a pixel a
// pixel, the lines that decorate text, of their patterns, and the
// interface's shapes, of rounded corners, borders and shadows. Every shape is an instance of a quad, whose four corners come from
// the vertex's index, drawn as a strip.
//
// Colors are sRGB's encoded components, premultiplied by their opacity, and
// the drawable's format keeps them encoded: blending mixes them as they are.
// A glyph's coverage can take the weight of macOS's text, which thins dark
// text on a light background where blending alone would not. A drawable
// that is not opaque lets what is behind the window show through.

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
// the atlas; which atlas, 0 for coverage's and 1 for colors'; whether its
// coverage takes the weight of the system's text, 1, or stays as drawn, 0;
// the color that fills a glyph of coverage.
struct Glyph {
    float2 position;
    float2 size;
    uint2 atlas;
    uint texture;
    uint weight;
    float4 color;
};

struct Fragment {
    float4 position [[position]];
    float4 color [[flat]];
    uint2 atlas [[flat]];
    uint texture [[flat]];
    // The exponent of a glyph's coverage: 1 for its coverage as drawn.
    float exponent [[flat]];
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
    out.exponent = 1.0;
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
    out.exponent = 1.0;
    out.local = float2(0);
    return out;
}

fragment float4 solid_fragment(Fragment in [[stage_in]]) {
    return in.color;
}

// The exponent of the coverage of a glyph of `color`, premultiplied, that
// gives its text the weight of the system's: 2 less the luminance of its
// color, as macOS thins dark text on a light background, and leaves light
// text on a dark one nearly as drawn, which blending the encoded
// components alone does not.
static float weight_exponent(float4 color) {
    if (color.a <= 0.0) {
        return 1.0;
    }
    float3 encoded = color.rgb / color.a;
    float3 linear = select(pow((encoded + 0.055) / 1.055, 2.4), encoded / 12.92,
                           encoded <= 0.04045);
    return 2.0 - dot(linear, float3(0.2126, 0.7152, 0.0722));
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
    out.exponent = glyph.weight == 1 && glyph.texture == 0 ? weight_exponent(glyph.color) : 1.0;
    out.local = local;
    return out;
}

// A glyph of coverage, filled with its color, which comes premultiplied; or
// of its own colors, premultiplied as the atlas holds them, of its color's
// opacity, as a fade gives it. The two kinds draw in one pass, each over
// those before it.
fragment float4 glyph_fragment(Fragment in [[stage_in]],
                               texture2d<float, access::read> coverage [[texture(0)]],
                               texture2d<float, access::read> colors [[texture(1)]]) {
    uint2 pixel = in.atlas + uint2(in.local);
    if (in.texture == 1) {
        return colors.read(pixel) * in.color.a;
    }
    float covered = coverage.read(pixel).r;
    if (in.exponent != 1.0) {
        covered = pow(covered, in.exponent);
    }
    return in.color * covered;
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
// shape itself; what fills it, or its shadow; its border's color; and the
// arc of its inscribed ring that it draws instead, its start and its span in
// turns clockwise from the top, its thickness, and 1, or 0 for none.
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
    float4 arc;
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
    float4 arc [[flat]];
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
    out.arc = shape.arc;
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
    float t = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    t *= t;
    return sign(x) * (1.0 - 1.0 / (t * t));
}

// The part of a pixel at `p`, from a ring's centre, that an arc covers: its
// line, of `thickness`, inside the circle of `outer`, from `start` of the
// turn clockwise for `span` of it, the drawable's y going down; its ends
// cut across the line, smoothed as its sides are.
static float arc_coverage(float2 p, float outer, float4 arc) {
    float thickness = min(arc.z, outer);
    float middle = outer - thickness / 2.0;
    float across = clamp(thickness / 2.0 + 0.5 - abs(length(p) - middle), 0.0, 1.0);
    float span = arc.y;
    if (span >= 1.0) {
        return across;
    }
    float turns = fract(atan2(p.x, -p.y) / (2.0 * M_PI_F) + 1.0);
    float t = fract(turns - arc.x + 1.0);
    float inside = t <= span ? min(t, span - t) : -min(t - span, 1.0 - t);
    float along = clamp(inside * 2.0 * M_PI_F * middle + 0.5, 0.0, 1.0);
    return across * along;
}

// A shadow covers a pixel as a Gaussian blur of the outline's edge, the
// error function across its distance; a shape, the part of the pixel within
// its outline, filled to it, and its border over the fill, between its
// outline and the border's inner one, so that a border not opaque shows the
// fill under it, as CSS's `border-box` has it.
fragment float4 shape_fragment(ShapeFragment in [[stage_in]]) {
    float2 p = in.position.xy - in.center;
    if (in.arc.w > 0.0) {
        return in.fill * arc_coverage(p, min(in.extent.x, in.extent.y), in.arc);
    }
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
    float ring = outer - inner;
    return in.border_color * ring + in.fill * (outer - in.border_color.a * ring);
}
