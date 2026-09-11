import simd
import FieldCore

extension Mesher {
    /// Leaves are parametric surfaces seeded on the compiled canopy field.
    /// Full geometry provides multisample coverage without alpha-test sparkle.
    public static func leaves(on surface:Mesh,count:Int=1800,flattened:Bool=false)->Mesh {
        var mesh=Mesh(),rng=SeededRandom(seed:72519),areas:[Float]=[],sum:Float=0
        func point(_ id:UInt32)->V3 {let p=surface.vertices[Int(id)].position;return V3(p.x,p.y,p.z)}
        for i in stride(from:0,to:surface.indices.count,by:3) {
            let a=point(surface.indices[i]),b=point(surface.indices[i+1]),c=point(surface.indices[i+2]);sum+=length(cross(b-a,c-a))*0.5;areas.append(sum)
        }
        for _ in 0..<count {
            let area=rng.next()*sum;var lo=0,hi=areas.count-1
            while lo<hi {let m=(lo+hi)/2;if areas[m]<area{lo=m+1}else{hi=m}}
            let ids=Array(surface.indices[(lo*3)..<(lo*3+3)]),a=point(ids[0]),b=point(ids[1]),c=point(ids[2])
            let u=sqrt(rng.next()),v=rng.next(),center=a*(1-u)+b*(u*(1-v))+c*(u*v)
            var n=normalize(cross(b-a,c-a));if !n.x.isFinite {continue}
            n=normalize(n+V3(rng.range(-0.4,0.4),rng.range(-0.1,0.5),rng.range(-0.4,0.4)))
            let tangent=normalize(cross(abs(n.y)<0.95 ? V3(0,1,0):V3(1,0,0),n)),bitangent=cross(n,tangent),angle=rng.range(0,6.283)
            let t=tangent*cos(angle)+bitangent*sin(angle),s=cross(n,t),p=center+n*rng.range(0.025,0.10)
            let length=rng.range(0.12,0.25),width=length*rng.range(0.4,0.65),color=V3(repeating:rng.range(0.85,1.15))
            let tip=p+t*length,root=p-t*length,left=p+s*width,right=p-s*width,ridge=p+n*0.035
            if flattened {
                // Keep all four boundary vertices, colors and leaf orientations.
                // Only the interior 35 mm ridge disappears at subpixel distance.
                mesh.triangle(Vertex(tip,n,color),Vertex(left,n,color),Vertex(root,n,color))
                mesh.triangle(Vertex(tip,n,color),Vertex(root,n,color),Vertex(right,n,color))
            } else {
                for (a,b) in [(tip,left),(left,root),(root,right),(right,tip)] {mesh.triangle(Vertex(ridge,n,color),Vertex(a,n,color),Vertex(b,n,color))}
            }
        }
        // The draw classifier adds the deformation Jacobian bound to this
        // undeformed Hausdorff error before applying the projected-error budget.
        if flattened {mesh.geometricError=0.035}
        return MeshProcessing.optimize(mesh)
    }
}
