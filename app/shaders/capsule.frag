#include <flutter/runtime_effect.glsl>
uniform vec2 size;
uniform sampler2D background;
out vec4 fragColor;

void main() {
  vec2 p = FlutterFragCoord().xy - size * 0.5;
  float radius = 0.5 * size.y;
  float axis = max(0.0, size.x * 0.5 - radius);
  vec2 normal = p - vec2(clamp(p.x, -axis, axis), 0.0);
  float d = length(normal) / radius;
  float rim = smoothstep(0.58, 0.94, d) * (1.0 - smoothstep(0.94, 1.0, d));
  vec2 uv = (FlutterFragCoord().xy - normal * (0.16 * rim)) / size;
#ifdef IMPELLER_TARGET_OPENGLES
  uv.y = 1.0 - uv.y;
#endif
  fragColor = texture(background, clamp(uv, vec2(0.0), vec2(1.0)));
}
