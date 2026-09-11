import FieldCore
import simd

package func perspective(_ fov: Float, _ aspect: Float, _ near: Float, _ far: Float)
  -> simd_float4x4
{
  let y = 1 / tan(fov / 2)
  let x = y / aspect
  let z = far / (near - far)
  return simd_float4x4(
    columns: (SIMD4(x, 0, 0, 0), SIMD4(0, y, 0, 0), SIMD4(0, 0, z, -1), SIMD4(0, 0, z * near, 0)))
}
package func lookAt(_ eye: V3, _ target: V3, up: V3 = V3(0, 1, 0)) -> simd_float4x4 {
  let z = normalize(eye - target)
  let x = normalize(cross(up, z))
  let y = cross(z, x)
  return simd_float4x4(
    columns: (
      SIMD4(x.x, y.x, z.x, 0), SIMD4(x.y, y.y, z.y, 0), SIMD4(x.z, y.z, z.z, 0),
      SIMD4(-dot(x, eye), -dot(y, eye), -dot(z, eye), 1)
    ))
}
package func orthographic(_ size: Float, _ near: Float, _ far: Float) -> simd_float4x4 {
  simd_float4x4(
    columns: (
      SIMD4(1 / size, 0, 0, 0), SIMD4(0, 1 / size, 0, 0), SIMD4(0, 0, 1 / (near - far), 0),
      SIMD4(0, 0, near / (near - far), 1)
    ))
}
