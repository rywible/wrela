#include "include/CGeometry.h"
#include "vendor/meshoptimizer.h"
#include <vector>
#include <cstring>
size_t sgOptimize(void *vertices,size_t count,uint32_t *indices,size_t indexCount) {
    std::vector<unsigned int> remap(count);
    size_t unique=meshopt_generateVertexRemap(remap.data(),indices,indexCount,vertices,count,48);
    std::vector<unsigned char> data(count*48);
    meshopt_remapVertexBuffer(data.data(),vertices,count,48,remap.data());
    meshopt_remapIndexBuffer(indices,indices,indexCount,remap.data());
    meshopt_optimizeVertexCache(indices,indices,indexCount,unique);
    return meshopt_optimizeVertexFetch(vertices,indices,indexCount,data.data(),unique,48);
}
size_t sgSimplify(uint32_t *out,const uint32_t *indices,size_t indexCount,const void *vertices,size_t vertexCount,float ratio,float error,float *resultError) {
    const float weights[3]={0.2f,0.2f,0.2f};
    return meshopt_simplifyWithAttributes(out,indices,indexCount,(const float*)vertices,vertexCount,48,(const float*)((const char*)vertices+16),48,weights,3,nullptr,size_t(indexCount*ratio)/3*3,error,meshopt_SimplifyLockBorder|meshopt_SimplifyErrorAbsolute,resultError);
}

size_t sgMeshletBound(size_t indexCount) {return meshopt_buildMeshletsBound(indexCount,64,124);}
size_t sgMeshlets(SGMeshlet *out,uint32_t *vertices,uint8_t *triangles,const uint32_t *indices,size_t indexCount,const void *positions,size_t vertexCount) {
    std::vector<meshopt_Meshlet> meshlets(sgMeshletBound(indexCount));
    size_t count=meshopt_buildMeshlets(meshlets.data(),vertices,triangles,indices,indexCount,(const float*)positions,vertexCount,48,64,124,0.0f);
    for(size_t i=0;i<count;i++) {
        const auto &m=meshlets[i];auto b=meshopt_computeMeshletBounds(vertices+m.vertex_offset,triangles+m.triangle_offset,m.triangle_count,(const float*)positions,vertexCount,48);
        out[i]={m.vertex_offset,m.triangle_offset,m.vertex_count,m.triangle_count,b.center[0],b.center[1],b.center[2],b.radius};
    }
    return count;
}
