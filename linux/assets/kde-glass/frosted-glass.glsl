// Adapted from OverShifted/LiquidGlass, assets/shaders/BatchRenderer2D.glsl.
// Copyright (c) 2026 Sepehr Kalanaki. Licensed under the MIT License;
// see LICENSE-OverShifted-LiquidGlass.

float liquidGlassRandom(vec2 coordinate)
{
    return fract(sin(dot(coordinate, vec2(12.9898, 78.233))) * 43758.5453);
}

GlassFragment snellsRefraction(vec2 position, vec2 halfBlurSize, vec4 cornerRadius, float minHalfSize, float dist, float edgeFactor, float concaveFactor)
{
    // The rim band is the part of the slab that bends light across the edge.
    float curvature = clamp(materialCurvature, 0.0, 3.0);
    float bevelWidth = clamp(minHalfSize * 0.45, 10.0, 72.0);

    // Fill the whole area KWin gives, rounded only by the corner radius it
    // passes for this surface. One exact rounded-rectangle distance field
    // drives the silhouette, the rim optics, and the rim lighting.
    vec4 radii = clamp(cornerRadius, vec4(0.0), vec4(minHalfSize));
    float radius = position.x > 0.0
        ? (position.y > 0.0 ? radii.y : radii.w)
        : (position.y > 0.0 ? radii.x : radii.z);
    vec2 corner = abs(position) - (halfBlurSize - vec2(radius));
    float shapeDistance = length(max(corner, 0.0))
        + min(max(corner.x, corner.y), 0.0) - radius;
    vec2 outward = corner.x > 0.0 && corner.y > 0.0
        ? normalize(corner)
        : (corner.x > corner.y ? vec2(1.0, 0.0) : vec2(0.0, 1.0));
    outward *= vec2(position.x < 0.0 ? -1.0 : 1.0, position.y < 0.0 ? -1.0 : 1.0);
    float rimDistance = max(0.0, -shapeDistance);
    float shapeCoverage = 1.0 - smoothstep(-0.5, 0.5, shapeDistance);

    // A band wider than its corner radius folds into a separate facet at the
    // corner. Keep the band wide along the edges and narrow it to the corner
    // radius as it approaches each corner, so the rim bends continuously.
    float alongEdge = max(0.0, -min(corner.x, corner.y));
    bevelWidth = mix(max(min(radius, bevelWidth), 2.0), bevelWidth,
        smoothstep(0.0, bevelWidth * 1.5, alongEdge));

    // The glass is a slab with a rounded, convex rim and a shallow dome over
    // its face. Their normals light the surface. The dome is a smooth oval
    // so the face has no creases of its own.
    float bevelHeight = bevelWidth * 0.85 * curvature;
    float bevel = clamp(rimDistance / bevelWidth, 0.02, 1.0);
    float bevelRise = 1.0 - bevel;
    float bevelProfile = sqrt(1.0 - bevelRise * bevelRise);
    float rimSlope = min(bevelHeight / bevelWidth * bevelRise / bevelProfile, 8.0);
    float domeHeight = min(minHalfSize * 0.06, 10.0) * curvature;
    vec2 domePosition = position / max(halfBlurSize, vec2(1.0));
    vec2 domeGradient = 2.0 * domeHeight * domePosition
        / max(halfBlurSize, vec2(1.0));
    vec3 normal = normalize(vec3(outward * rimSlope + domeGradient, 1.0));

    // Rounded glass resting on content magnifies its face and compresses
    // its rim: the image swells toward the middle and wraps into the edge
    // while staying continuous with the content outside. Draw each point
    // toward the center, fading along the rim profile to nothing at the
    // edge. Lines therefore bow toward the corners instead of stretching.
    float ior = clamp(materialIOR, 1.0, 2.5);
    float effectPower = clamp(refractionStrength / 0.75, 0.0, 2.0);
    // The magnification is the same in both directions so text on the face
    // keeps its proportions; only the rim compresses.
    vec2 displacement = -position * 0.12 * bevelProfile * effectPower;

    // Optional dispersion: short wavelengths bend slightly more. The rim
    // compresses the background strongly, so even a small split becomes a
    // wide colored streak; cap it in background pixels.
    float fringing = clamp(refractionRGBFringing, 0.0, 1.0);
    float dispersion = min(fringing * 0.3,
        fringing * 20.0 / max(length(displacement), 1.0));
    vec2 texel = 1.0 / max(blurSize, vec2(1.0));
    vec2 baseUV = position * texel + vec2(0.5);
    vec2 minUV = 0.5 * texel;
    vec2 maxUV = vec2(1.0) - 0.5 * texel;
    vec2 sampleUV = clamp(baseUV + displacement * texel, minUV, maxUV);
    vec4 color = texture(texUnit, sampleUV);
    color.r = texture(texUnit, clamp(baseUV
        + displacement * (1.0 - dispersion) * texel, minUV, maxUV)).r;
    color.b = texture(texUnit, clamp(baseUV
        + displacement * (1.0 + dispersion) * texel, minUV, maxUV)).b;

    // A very small symmetric diffusion gives the surface microscopic
    // roughness without replacing the coherent refracted image with blur.
    vec2 roughnessOffset = vec2(1.25, -0.85) * texel;
    vec4 roughTransmission = 0.5 * (
        texture(texUnit, clamp(sampleUV + roughnessOffset, minUV, maxUV))
        + texture(texUnit, clamp(sampleUV - roughnessOffset, minUV, maxUV))
    );
    color.rgb = mix(color.rgb, roughTransmission.rgb,
        clamp(materialRoughness, 0.0, 0.16));

    // KWin draws this glass before the window's content, which is mostly
    // light text and icons. A continuous tone curve dims the glass in
    // proportion to the square of the surrounding background brightness:
    // dark areas are barely touched and bright ones come down the most, so
    // that content stays readable without a threshold. Wide taps follow the
    // neighborhood rather than fine detail; lighting is applied afterward so
    // the rim and highlights keep their brightness.
    vec2 readabilityReach = 28.0 * texel;
    vec3 neighborhood = 0.25 * (
        texture(texUnit, clamp(baseUV + vec2(readabilityReach.x, 0.0), minUV, maxUV)).rgb
        + texture(texUnit, clamp(baseUV - vec2(readabilityReach.x, 0.0), minUV, maxUV)).rgb
        + texture(texUnit, clamp(baseUV + vec2(0.0, readabilityReach.y), minUV, maxUV)).rgb
        + texture(texUnit, clamp(baseUV - vec2(0.0, readabilityReach.y), minUV, maxUV)).rgb);
    float backgroundLuminance = dot(max(neighborhood, color.rgb),
        vec3(0.2126, 0.7152, 0.0722));
    color.rgb *= 1.0 - 0.44 * backgroundLuminance * backgroundLuminance;

    // Light the curved surface from the top left. Shading follows the real
    // surface normal, so the dome brightens toward the light and the rim
    // facing away falls into a soft shade.
    vec3 keyLight = normalize(vec3(-0.70, 0.70, 0.72));
    vec3 view = vec3(0.0, 0.0, 1.0);
    float facing = dot(normal, keyLight) - keyLight.z;
    color.rgb *= 1.0 + 0.22 * facing;
    // The real dome is too shallow to light the face visibly over a dark
    // desktop, so a broader copy of it lifts the top left and shades the
    // bottom right.
    vec3 bodyNormal = normalize(vec3(domePosition * 0.9, 1.0));
    float bodyLight = smoothstep(-0.20, 0.92, dot(bodyNormal, keyLight));
    color.rgb *= 1.0 - 0.09 * (1.0 - bodyLight);
    color.rgb = mix(color.rgb, vec3(1.0), 0.060 * bodyLight);

    // Glass reflects more at grazing angles, so the steep rim mirrors a soft
    // bright environment on every side. This is what gives the edge depth.
    float cosine = clamp(normal.z, 0.0, 1.0);
    float fresnel = 0.04 + 0.96 * pow(1.0 - cosine, 5.0);
    vec3 reflected = reflect(-view, normal);
    float environment = 0.30 + 0.70 * smoothstep(-0.70, 0.90, dot(reflected, keyLight));
    color.rgb = mix(color.rgb, vec3(environment), clamp(fresnel, 0.0, 0.45));

    // Light catches the steep part of the rim as a thin line. It is
    // brightest where the rim faces the light and reappears, weaker, on the
    // opposite rim where light leaves the glass after crossing it. A broad
    // angular falloff keeps the line continuous along straight edges.
    float steepness = smoothstep(0.30, 0.85, 1.0 - cosine);
    float lightAlignment = dot(outward, normalize(keyLight.xy));
    float keyLobe = pow(smoothstep(-0.35, 1.0, lightAlignment), 2.0);
    float exitLobe = pow(smoothstep(-0.35, 1.0, -lightAlignment), 2.0);
    float rimLight = steepness * (0.04 + 0.47 * keyLobe + 0.085 * exitLobe);
    // Light entering through the lit shoulder spreads into the rim, and the
    // curved body gathers it again just inside the opposite edge.
    float shoulderGlow = 0.085 * keyLobe * bevelRise * bevelRise;
    float exitCaustic = 0.04 * exitLobe * pow(bevelRise, 1.5)
        * (1.0 - steepness);
    color.rgb = mix(color.rgb, vec3(1.0),
        clamp(rimLight + shoulderGlow + exitCaustic, 0.0, 0.65));

    // Dither below one 8-bit step to avoid banding without visible grain.
    color.rgb += vec3(liquidGlassRandom(gl_FragCoord.xy) - 0.5) / 255.0;

    color.rgb = clamp(color.rgb, 0.0, 1.0);

    // glass() applies the remaining material stages, then premultiplies this
    // analytical coverage for KWin's GL_ONE/GL_ONE_MINUS_SRC_ALPHA blend.
    return GlassFragment(vec4(color.rgb, shapeCoverage), dist, edgeFactor,
        concaveFactor, normal, ior);
}
