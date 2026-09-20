// Adapted from OverShifted/LiquidGlass, assets/shaders/BatchRenderer2D.glsl.
// Copyright (c) 2026 Sepehr Kalanaki. Licensed under the MIT License;
// see LICENSE-OverShifted-LiquidGlass.

const float liquidGlassE = 2.718281828459045;

float liquidGlassCurve(float distance)
{
    const float a = 0.7;
    const float b = 2.3;
    const float c = 5.2;
    const float d = 6.9;
    return 1.0 - b * pow(c * liquidGlassE, -d * distance - a);
}

float liquidGlassRandom(vec2 coordinate)
{
    return fract(sin(dot(coordinate, vec2(12.9898, 78.233))) * 43758.5453);
}

float liquidGlassRoundedRadialPosition(vec2 position, vec2 halfSize, float radius)
{
    vec2 absolutePosition = abs(position);
    float positionLength = length(absolutePosition);
    if (positionLength < 0.0001) {
        return 0.0;
    }

    vec2 ray = absolutePosition / positionLength;
    vec2 cornerCenter = max(halfSize - vec2(radius), vec2(0.0));
    float verticalHit = halfSize.x / max(ray.x, 0.0001);
    if (verticalHit * ray.y <= cornerCenter.y) {
        return positionLength / verticalHit;
    }

    float horizontalHit = halfSize.y / max(ray.y, 0.0001);
    if (horizontalHit * ray.x <= cornerCenter.x) {
        return positionLength / horizontalHit;
    }

    float projectedCenter = dot(ray, cornerCenter);
    float discriminant = max(projectedCenter * projectedCenter
        - dot(cornerCenter, cornerCenter) + radius * radius, 0.0);
    float cornerHit = projectedCenter + sqrt(discriminant);
    return positionLength / max(cornerHit, 0.0001);
}

GlassFragment snellsRefraction(vec2 position, vec2 halfBlurSize, vec4 cornerRadius, float minHalfSize, float dist, float edgeFactor, float concaveFactor)
{
    vec2 p = position / max(halfBlurSize, vec2(1.0));
    float maxHalfSize = max(halfBlurSize.x, halfBlurSize.y);
    float aspectRatio = maxHalfSize / max(minHalfSize, 1.0);
    // Derive rounding from the allocated surface. Thin surfaces naturally
    // become pills; larger, near-square surfaces retain most of their corners.
    float radiusFraction = mix(0.14, 1.0,
        smoothstep(1.35, 2.75, aspectRatio));
    float dynamicRadius = minHalfSize * radiusFraction;
    // Normalize every point by the rounded boundary reached along its ray
    // from the center. One field now drives silhouette, optics, and lighting.
    float radialPosition = liquidGlassRoundedRadialPosition(position,
        halfBlurSize, dynamicRadius);
    float shapeDistance = radialPosition - 1.0;
    float shapeAA = max(fwidth(shapeDistance), 1.0 / max(minHalfSize, 1.0));
    float shapeCoverage = 1.0 - smoothstep(-shapeAA, shapeAA, shapeDistance);
    float interiorDistance = max(0.0, 1.0 - radialPosition);

    // OverShifted's default fPower is 1.0. Keep the KWin strength control by
    // making its configured default (15 -> shader value 0.75) map to that.
    float effectPower = clamp(refractionStrength / 0.75, 0.0, 2.0);
    float radialScale = pow(max(0.0001, liquidGlassCurve(interiorDistance)), effectPower);
    vec2 samplePosition = p * radialScale;
    vec2 sampleUV = clamp(samplePosition * 0.5 + vec2(0.5),
        0.5 / blurSize, 1.0 - 0.5 / blurSize);

    // The reference implementation refracts its lightly blurred framebuffer,
    // then adds subtle grain before applying the edge glow.
    vec4 color = texture(texUnit, sampleUV);
    // A very small symmetric diffusion gives the surface microscopic
    // roughness without replacing the coherent refracted image with blur.
    vec2 roughnessOffset = vec2(1.25, -0.85) / blurSize;
    vec4 roughTransmission = 0.5 * (
        texture(texUnit, clamp(sampleUV + roughnessOffset, 0.0, 1.0))
        + texture(texUnit, clamp(sampleUV - roughnessOffset, 0.0, 1.0))
    );
    color = mix(color, roughTransmission, clamp(materialRoughness, 0.0, 0.16));
    color.rgb += vec3(liquidGlassRandom(gl_FragCoord.xy * 0.001) - 0.5) * 0.06;

    float angularGlow = sin(atan(p.y, p.x) - 0.5);
    float edgeGlow = 1.0 - smoothstep(-0.5, 0.5, interiorDistance);
    color.rgb *= 1.0 + angularGlow * 0.25 * edgeGlow;

    // KWin composites this material directly over the desktop, whereas the
    // reference demo renders into a lit scene. Restore that missing scene
    // light with the analytical rounded-surface normal. Keep illumination in a
    // shell that follows the signed-distance contour instead of laying a
    // circular-looking gradient across the front face.
    const float gradientStep = 1.0;
    vec2 surfaceGradient = vec2(
        liquidGlassRoundedRadialPosition(position + vec2(gradientStep, 0.0),
            halfBlurSize, dynamicRadius)
            - liquidGlassRoundedRadialPosition(position - vec2(gradientStep, 0.0),
                halfBlurSize, dynamicRadius),
        liquidGlassRoundedRadialPosition(position + vec2(0.0, gradientStep),
            halfBlurSize, dynamicRadius)
            - liquidGlassRoundedRadialPosition(position - vec2(0.0, gradientStep),
                halfBlurSize, dynamicRadius)
    ) * minHalfSize * 0.5;
    vec3 lightNormal = normalize(vec3(surfaceGradient * 2.2, 1.0));
    vec3 keyLight = normalize(vec3(-0.70, 0.70, 0.72));
    float diffuseLight = max(dot(lightNormal, keyLight), 0.0);
    vec2 planarNormal = length(surfaceGradient) > 0.0001
        ? normalize(surfaceGradient) : vec2(0.0);
    float oppositeLight = smoothstep(0.05, 0.90,
        dot(planarNormal, -normalize(keyLight.xy)));
    // Place the strongest light just inside the silhouette, on the shoulder
    // of the lens. This reads as a three-quarter bevel rather than an outline
    // painted directly on the edge or a gradient spread over the front face.
    float innerReach = 1.0 - smoothstep(0.16, 0.52, interiorDistance);
    float edgeRelease = smoothstep(0.0, 0.10, interiorDistance);
    float shoulderLight = innerReach * mix(0.38, 1.0, edgeRelease);
    float edgeKiss = 1.0 - smoothstep(0.0, 0.075, interiorDistance);
    float lightProfile = max(shoulderLight, 0.82 * edgeKiss);
    // Compensate for analytical alpha falloff at the silhouette. Without this
    // narrow term the premultiplied highlight appears to stop one pixel early.
    float silhouetteKiss = 1.0 - smoothstep(0.0, 0.11 + shapeAA,
        interiorDistance);
    float coverageCompensation = mix(1.0, 2.2, 1.0 - shapeCoverage);
    float transmittedRim = (0.018 + 0.070 * diffuseLight)
        * pow(lightProfile, 1.35);
    float rimHighlight = 0.24 * pow(diffuseLight, 2.5)
        * pow(lightProfile, 2.4);
    float edgeSpecular = 0.16 * pow(diffuseLight, 2.0)
        * silhouetteKiss * coverageCompensation;
    color.rgb = mix(color.rgb, vec3(1.0),
        clamp(transmittedRim + rimHighlight + edgeSpecular, 0.0, 0.38));
    float respondingShadow = (0.040 + 0.26 * oppositeLight)
        * pow(lightProfile, 1.5);
    color.rgb *= 1.0 - respondingShadow;

    float bend = clamp(1.0 - radialScale, 0.0, 1.0);
    vec3 normal = normalize(vec3(planarNormal * bend * 3.0, 1.0));
    // glass() applies the remaining material stages, then premultiplies this
    // analytical coverage for KWin's GL_ONE/GL_ONE_MINUS_SRC_ALPHA blend.
    return GlassFragment(vec4(color.rgb, shapeCoverage), dist, edgeFactor,
        concaveFactor, normal, materialIOR);
}
