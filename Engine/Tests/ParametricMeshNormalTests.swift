import XCTest
import simd
@testable import FieldCore
@testable import FieldCompiler

final class ParametricMeshNormalTests: XCTestCase {
  // Includes millimetre details, the 35 mm seed, ordinary objects and 100 m content.
  private let sizes: [Float] = [0.001, 0.035, 1, 100]

  private func xyz(_ value: SIMD4<Float>) -> V3 { V3(value.x, value.y, value.z) }

  private func assertUnit(_ normal: V3, file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertTrue(normal.x.isFinite && normal.y.isFinite && normal.z.isFinite, file: file, line: line)
    XCTAssertEqual(length(normal), 1, accuracy: 0.00001, file: file, line: line)
  }

  func testTiltedPlaneNormalAndWindingAreIndependentOfMetreScale() {
    let a = V3(0.8, 0.25, 0.10), b = V3(0.10, 0.30, 0.7)
    let expected = normalize(cross(a, b))
    for size in sizes {
      let mesh = ParametricMesh.surface(u: 4, v: 4) { u, v in size * (a * u + b * v) }
      for vertex in mesh.vertices {
        let normal = xyz(vertex.normal)
        assertUnit(normal)
        XCTAssertLessThan(length(normal - expected), 0.001, "Plane size \(size)")
      }
      for index in stride(from: 0, to: mesh.indices.count, by: 3) {
        let p = (0..<3).map { xyz(mesh.vertices[Int(mesh.indices[index + $0])].position) }
        XCTAssertGreaterThan(dot(cross(p[1] - p[0], p[2] - p[0]), expected), 0)
      }
    }
  }

  func testCurvedSurfaceNormalsAndOffsetsFollowSphereAtEveryScale() {
    for size in sizes {
      let radius = size * 0.5
      func position(_ u: Float, _ v: Float) -> V3 {
        let longitude = u * 2 * Float.pi
        // Stay just off the singular parameter poles; include the seam/end rows.
        let latitude = (0.0001 + v * 0.9998) * Float.pi
        return radius * V3(sin(latitude) * cos(longitude), cos(latitude),
          sin(latitude) * sin(longitude))
      }
      let mesh = ParametricMesh.surface(u: 32, v: 16, position: position)
      for vertex in mesh.vertices {
        let normal = xyz(vertex.normal), expected = normalize(xyz(vertex.position))
        assertUnit(normal)
        XCTAssertLessThan(length(normal - expected), 0.002, "Sphere diameter \(size)")
      }
      let samples: [(Float, Float)] = [(0, 0.35), (0.27, 0.62), (0.75, 0.5), (1, 0.75)]
      for (u, v) in samples {
        let origin = position(u, v), expected = normalize(origin)
        for sign: Float in [-1, 1] {
          let distance = sign * size * 0.025
          let point = ParametricMesh.offsetPoint(u: u, v: v, distance: distance, position: position)
          XCTAssertLessThan(length((point - origin) - expected * distance), size * 0.00005,
            "Signed sphere offset at diameter \(size)")
        }
      }
    }
  }

  func testEllipsoidNormalsMatchImplicitGradientIncludingPolesAndSeam() {
    for size in sizes {
      let radii = V3(0.5, 0.35, 0.15) * size
      let detail = 16, columns = detail * 2 + 1
      let mesh = ParametricMesh.ellipsoid(.zero, radii, detail: detail)
      for vertex in mesh.vertices {
        // Gradient of x²/a² + y²/b² + z²/c² = 1.
        let expected = normalize(xyz(vertex.position) / (radii * radii))
        let normal = xyz(vertex.normal)
        assertUnit(normal)
        XCTAssertLessThan(length(normal - expected), 0.00001, "Ellipsoid size \(size)")
      }
      for row in 0...detail {
        let first = xyz(mesh.vertices[row * columns].normal)
        let last = xyz(mesh.vertices[row * columns + columns - 1].normal)
        XCTAssertLessThan(length(first - last), 0.00001)
      }
      XCTAssertGreaterThan(mesh.vertices[0].normal.y, 0.9999)
      XCTAssertLessThan(mesh.vertices[detail * columns].normal.y, -0.9999)
    }
  }

  func testAnalyticEllipsoidNormalsSurviveTranslationAndPreserveMirroredWinding() {
    let radii = V3(0.0175, 0.012, 0.004)
    let local = ParametricMesh.ellipsoid(.zero, radii, detail: 12)
    let translated = ParametricMesh.ellipsoid(V3(1_000, 200, -600), radii, detail: 12)
    // The analytic source retains its normals even where Float world positions
    // lose fine derivative samples. This does not promise unrounded positions.
    for index in local.vertices.indices {
      assertUnit(xyz(translated.vertices[index].normal))
      XCTAssertEqual(local.vertices[index].normal, translated.vertices[index].normal)
    }
    let mirrored = ParametricMesh.ellipsoid(.zero, V3(-0.5, 0.35, 0.15), detail: 12)
    for index in stride(from: 0, to: mirrored.indices.count, by: 3) {
      let vertices = (0..<3).map { mirrored.vertices[Int(mirrored.indices[index + $0])] }
      let face = cross(xyz(vertices[1].position - vertices[0].position),
        xyz(vertices[2].position - vertices[0].position))
      let normal = xyz(vertices[0].normal + vertices[1].normal + vertices[2].normal)
      XCTAssertGreaterThan(dot(face, normal), 0)
    }
  }

  func testTubeNormalsFollowCylinderAtEveryScale() {
    for size in sizes {
      let mesh = ParametricMesh.tube(segments: 32, sides: 16,
        center: { V3(0, $0 * size, 0) }, radius: { _ in size * 0.15 })
      for vertex in mesh.vertices {
        let normal = xyz(vertex.normal)
        let expected = normalize(V3(vertex.position.x, 0, vertex.position.z))
        assertUnit(normal)
        XCTAssertLessThan(length(normal - expected), 0.00001, "Tube size \(size)")
      }
    }
  }

  func testCollapsedFramesKeepFiniteFallbackWithoutInventingOffsetDirection() {
    for size in sizes {
      let point = V3(size, -size, size * 0.5)
      let mesh = ParametricMesh.surface(u: 2, v: 2) { _, _ in point }
      XCTAssertTrue(mesh.vertices.allSatisfy { $0.normal == SIMD4(0, 1, 0, 0) })
      XCTAssertEqual(ParametricMesh.offsetPoint(u: 0.5, v: 0.5, distance: size) { _, _ in point }, point)
    }
    let line = ParametricMesh.surface(u: 2, v: 2) { u, v in V3(u + v, 0, 0) }
    XCTAssertTrue(line.vertices.allSatisfy { $0.normal == SIMD4(0, 1, 0, 0) })
    let collapsedTube = ParametricMesh.tube(segments: 4, sides: 8,
      center: { _ in .zero }, radius: { _ in 0 })
    XCTAssertTrue(collapsedTube.vertices.allSatisfy {
      $0.position == SIMD4(0, 0, 0, 1) && $0.normal == SIMD4(0, 1, 0, 0)
    })
  }
}
