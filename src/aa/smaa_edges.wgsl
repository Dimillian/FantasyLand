/**
 * Copyright (C) 2013 Jorge Jimenez (jorge@iryoku.com)
 * Copyright (C) 2013 Jose I. Echevarria (joseignacioechevarria@gmail.com)
 * Copyright (C) 2013 Belen Masia (bmasia@unizar.es)
 * Copyright (C) 2013 Fernando Navarro (fernandn@microsoft.com)
 * Copyright (C) 2013 Diego Gutierrez (diegog@unizar.es)
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
 * of the Software, and to permit persons to whom the Software is furnished to
 * do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software. As clarification, there
 * is no requirement that the copyright notice and permission be included in
 * binary distributions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
// SMAA 1x High, adapted from Bevy v0.17.0. See README.md in this directory.
struct SmaaInfo {
    rt_metrics: vec4<f32>,
}

struct VertexVaryings {
    clip_coord: vec2<f32>,
    tex_coord: vec2<f32>,
}

struct EdgeDetectionVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) offset_0: vec4<f32>,
    @location(1) offset_1: vec4<f32>,
    @location(2) offset_2: vec4<f32>,
    @location(3) tex_coord: vec2<f32>,
}

struct BlendingWeightCalculationVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) offset_0: vec4<f32>,
    @location(1) offset_1: vec4<f32>,
    @location(2) offset_2: vec4<f32>,
    @location(3) tex_coord: vec2<f32>,
}

struct NeighborhoodBlendingVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) offset: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
}

@group(0) @binding(0) var color_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> smaa_info: SmaaInfo;

@group(1) @binding(0) var color_sampler: sampler;



//-----------------------------------------------------------------------------
// SMAA Presets

const SMAA_THRESHOLD: f32 = 0.1;
const SMAA_MAX_SEARCH_STEPS: u32 = 16u;
const SMAA_MAX_SEARCH_STEPS_DIAG: u32 = 8u;
const SMAA_CORNER_ROUNDING: u32 = 25u;

//-----------------------------------------------------------------------------
// Configurable Defines

/**
 * SMAA_THRESHOLD specifies the threshold or sensitivity to edges.
 * Lowering this value you will be able to detect more edges at the expense of
 * performance.
 *
 * Range: [0, 0.5]
 *   0.1 is a reasonable value, and allows to catch most visible edges.
 *   0.05 is a rather overkill value, that allows to catch 'em all.
 *
 *   If temporal supersampling is used, 0.2 could be a reasonable value, as low
 *   contrast edges are properly filtered by just 2x.
 */
// (In the WGSL version of this shader, `SMAA_THRESHOLD` is set above, in "SMAA
// Presets".)

/**
 * SMAA_MAX_SEARCH_STEPS specifies the maximum steps performed in the
 * horizontal/vertical pattern searches, at each side of the pixel.
 *
 * In number of pixels, it's actually the double. So the maximum line length
 * perfectly handled by, for example 16, is 64 (by perfectly, we meant that
 * longer lines won't look as good, but still antialiased).
 *
 * Range: [0, 112]
 */
// (In the WGSL version of this shader, `SMAA_MAX_SEARCH_STEPS` is set above, in
// "SMAA Presets".)

/**
 * SMAA_MAX_SEARCH_STEPS_DIAG specifies the maximum steps performed in the
 * diagonal pattern searches, at each side of the pixel. In this case we jump
 * one pixel at time, instead of two.
 *
 * Range: [0, 20]
 *
 * On high-end machines it is cheap (between a 0.8x and 0.9x slower for 16 
 * steps), but it can have a significant impact on older machines.
 *
 * Define SMAA_DISABLE_DIAG_DETECTION to disable diagonal processing.
 */
// (In the WGSL version of this shader, `SMAA_MAX_SEARCH_STEPS_DIAG` is set
// above, in "SMAA Presets".)

/**
 * SMAA_CORNER_ROUNDING specifies how much sharp corners will be rounded.
 *
 * Range: [0, 100]
 *
 * Define SMAA_DISABLE_CORNER_DETECTION to disable corner processing.
 */
// (In the WGSL version of this shader, `SMAA_CORNER_ROUNDING` is set above, in
// "SMAA Presets".)

/**
 * If there is a neighbor edge that has SMAA_LOCAL_CONTRAST_FACTOR times
 * bigger contrast than current edge, current edge will be discarded.
 *
 * This allows to eliminate spurious crossing edges, and is based on the fact
 * that, if there is too much contrast in a direction, that will hide
 * perceptually contrast in the other neighbors.
 */
const SMAA_LOCAL_CONTRAST_ADAPTATION_FACTOR: f32 = 2.0;

//-----------------------------------------------------------------------------
// Non-Configurable Defines

const SMAA_AREATEX_MAX_DISTANCE: f32 = 16.0;
const SMAA_AREATEX_MAX_DISTANCE_DIAG: f32 = 20.0;
const SMAA_AREATEX_PIXEL_SIZE: vec2<f32> = (1.0 / vec2<f32>(160.0, 560.0));
const SMAA_AREATEX_SUBTEX_SIZE: f32 = (1.0 / 7.0);
const SMAA_SEARCHTEX_SIZE: vec2<f32> = vec2(66.0, 33.0);
const SMAA_SEARCHTEX_PACKED_SIZE: vec2<f32> = vec2(64.0, 16.0);

const SMAA_CORNER_ROUNDING_NORM: f32 = f32(SMAA_CORNER_ROUNDING) / 100.0;

//-----------------------------------------------------------------------------
// WGSL-Specific Functions

// This vertex shader produces the following, when drawn using indices 0..3:
//
//  1 |  0-----x.....2
//  0 |  |  s  |  . ´
// -1 |  x_____x´
// -2 |  :  .´
// -3 |  1´
//    +---------------
//      -1  0  1  2  3
//
// The axes are clip-space x and y. The region marked s is the visible region.
// The digits in the corners of the right-angled triangle are the vertex
// indices.
//
// The top-left has UV 0,0, the bottom-left has 0,2, and the top-right has 2,0.
// This means that the UV gets interpolated to 1,1 at the bottom-right corner
// of the clip-space rectangle that is at 1,-1 in clip space.
fn calculate_vertex_varyings(vertex_index: u32) -> VertexVaryings {
    // See the explanation above for how this works
    let uv = vec2<f32>(f32(vertex_index >> 1u), f32(vertex_index & 1u)) * 2.0;
    let clip_position = vec2<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0));

    return VertexVaryings(clip_position, uv);
}

//-----------------------------------------------------------------------------
// Vertex Shaders


/**
 * Edge Detection Vertex Shader
 */
@vertex
fn edge_detection_vertex_main(@builtin(vertex_index) vertex_index: u32) -> EdgeDetectionVaryings {
    let varyings = calculate_vertex_varyings(vertex_index);

    var edge_detection_varyings = EdgeDetectionVaryings();
    edge_detection_varyings.position = vec4(varyings.clip_coord, 0.0, 1.0);
    edge_detection_varyings.tex_coord = varyings.tex_coord;

    edge_detection_varyings.offset_0 = smaa_info.rt_metrics.xyxy * vec4(-1.0, 0.0, 0.0, -1.0) +
        varyings.tex_coord.xyxy;
    edge_detection_varyings.offset_1 = smaa_info.rt_metrics.xyxy * vec4(1.0, 0.0, 0.0, 1.0) +
        varyings.tex_coord.xyxy;
    edge_detection_varyings.offset_2 = smaa_info.rt_metrics.xyxy * vec4(-2.0, 0.0, 0.0, -2.0) +
        varyings.tex_coord.xyxy;

    return edge_detection_varyings;
}




//-----------------------------------------------------------------------------
// Edge Detection Pixel Shaders (First Pass)


/**
 * Luma Edge Detection
 *
 * IMPORTANT NOTICE: luma edge detection requires gamma-corrected colors, and
 * thus 'color_texture' should be a non-sRGB texture.
 */
@fragment
fn luma_edge_detection_fragment_main(in: EdgeDetectionVaryings) -> @location(0) vec4<f32> {
    // Calculate the threshold:
    // TODO: Predication.
    let threshold = vec2(SMAA_THRESHOLD);

    // Calculate luma:
    let weights = vec3(0.2126, 0.7152, 0.0722);
    let L = dot(textureSample(color_texture, color_sampler, in.tex_coord).rgb, weights);

    let Lleft = dot(textureSample(color_texture, color_sampler, in.offset_0.xy).rgb, weights);
    let Ltop  = dot(textureSample(color_texture, color_sampler, in.offset_0.zw).rgb, weights);

    // We do the usual threshold:
    var delta: vec4<f32> = vec4(abs(L - vec2(Lleft, Ltop)), 0.0, 0.0);
    var edges = step(threshold, delta.xy);

    // Then discard if there is no edge:
    if (dot(edges, vec2(1.0)) == 0.0) {
        discard;
    }

    // Calculate right and bottom deltas:
    let Lright  = dot(textureSample(color_texture, color_sampler, in.offset_1.xy).rgb, weights);
    let Lbottom = dot(textureSample(color_texture, color_sampler, in.offset_1.zw).rgb, weights);
    delta = vec4(delta.xy, abs(L - vec2(Lright, Lbottom)));

    // Calculate the maximum delta in the direct neighborhood:
    var max_delta = max(delta.xy, delta.zw);

    // Calculate left-left and top-top deltas:
    let Lleftleft = dot(textureSample(color_texture, color_sampler, in.offset_2.xy).rgb, weights);
    let Ltoptop   = dot(textureSample(color_texture, color_sampler, in.offset_2.zw).rgb, weights);
    delta = vec4(delta.xy, abs(vec2(Lleft, Ltop) - vec2(Lleftleft, Ltoptop)));

    // Calculate the final maximum delta:
    max_delta = max(max_delta.xy, delta.zw);
    let final_delta = max(max_delta.x, max_delta.y);

    // Local contrast adaptation:
    edges *= step(vec2(final_delta), SMAA_LOCAL_CONTRAST_ADAPTATION_FACTOR * delta.xy);

    return vec4(edges, 0.0, 1.0);
}



