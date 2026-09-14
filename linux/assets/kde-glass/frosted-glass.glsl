GlassFragment snellsRefraction(vec2 position, vec2 halfBlurSize, vec4 cornerRadius, float minHalfSize, float dist, float edgeFactor, float concaveFactor)
{
    // One continuous curved profile, with neither a saturated rim nor an inner cutoff.
    vec2 lensPosition = position / max(halfBlurSize, vec2(1.0));
    vec2 reach = max(vec2(1.0), min(vec2(edgeSizePixels), halfBlurSize * 0.85));
    vec2 inwardDistance = max(vec2(0.0), vec2(1.0) - lensPosition * lensPosition) * halfBlurSize;
    vec2 curvedSlope = lensPosition * exp(-inwardDistance / reach);
    float edgeAmount = min(1.0, length(curvedSlope));
    vec3 normal = normalize(vec3(curvedSlope * (1.2 + materialCurvature), 1.0));
    vec3 tangent = normalize(vec3(normal.z, 0.0, -normal.x));
    vec3 bitangent = cross(normal, tangent);
    vec3 centralRay = refract(vec3(0.0, 0.0, -1.0), normal, 1.0 / materialIOR);
    vec2 centralSlope = centralRay.xy / max(0.25, -centralRay.z);
    float opticalContrast = max(0.04, (materialIOR - 1.0) / materialIOR);

    // Normalize the rough transmission lobe so low IOR does not erase the frosted finish.
    // IOR still controls the mean ray; the whole surface shares this same material.
    vec4 transmitted = vec4(0.0);
    const int sampleCount = 48;
    for (int index = 0; index < sampleCount; ++index) {
        float radius = sqrt((float(index) + 0.5) / float(sampleCount));
        float angle = float(index) * 2.39996323;
        vec2 slope = vec2(cos(angle), sin(angle)) * radius * materialRoughness * 1.8;
        vec3 microNormal = normalize(normal + tangent * slope.x + bitangent * slope.y);
        vec3 ray = refract(vec3(0.0, 0.0, -1.0), microNormal, 1.0 / materialIOR);
        vec2 raySlope = ray.xy / max(0.25, -ray.z);
        vec2 roughSlope = (raySlope - centralSlope) * (0.3 / opticalContrast);
        vec2 offset = (centralSlope + roughSlope) * materialThickness / blurSize;
        vec2 sampleUV = clamp(uv + offset, 0.5 / blurSize, 1.0 - 0.5 / blurSize);
        transmitted += texture(sceneTexture, sampleUV);
    }
    transmitted /= float(sampleCount);
    transmitted.rgb *= 1.0 - materialInteriorShadow * (0.85 + 0.15 * edgeAmount);
    return GlassFragment(transmitted, dist, edgeFactor, concaveFactor, normal, materialIOR);
}
