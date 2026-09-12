#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
size_t sgOptimize(void *vertices, size_t vertexCount, uint32_t *indices, size_t indexCount,size_t vertexStride);
size_t sgSimplify(uint32_t *destination,const uint32_t *indices,size_t indexCount,const void *vertices,size_t vertexCount,float ratio,float error,float *resultError,size_t vertexStride);
typedef struct {uint32_t vertexOffset,triangleOffset,vertexCount,triangleCount;float x,y,z,radius;} SGMeshlet;
size_t sgMeshletBound(size_t indexCount);
size_t sgMeshlets(SGMeshlet *out,uint32_t *vertices,uint8_t *triangles,const uint32_t *indices,size_t indexCount,const void *positions,size_t vertexCount,size_t vertexStride);
#ifdef __cplusplus
}
#endif
