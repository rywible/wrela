FROM --platform=linux/amd64 nvidia/cuda:12.4.1-devel-ubuntu22.04

ARG DEBIAN_FRONTEND=noninteractive
ARG HY3D_REPO=https://github.com/Tencent-Hunyuan/Hunyuan3D-2.1.git
ARG HY3D_COMMIT=82920d643c0dc2f7bfd7255f45f62d386edfe60c

ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1
ENV PIP_NO_CACHE_DIR=1
ENV CUDA_HOME=/usr/local/cuda
ENV PATH=/usr/local/cuda/bin:${PATH}
ENV LD_LIBRARY_PATH=/usr/local/cuda/lib64:${LD_LIBRARY_PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    cmake \
    curl \
    ffmpeg \
    git \
    git-lfs \
    libegl1 \
    libeigen3-dev \
    libgconf-2-4 \
    libgl1 \
    libgl1-mesa-dev \
    libglib2.0-0 \
    libgomp1 \
    libsm6 \
    libxext6 \
    libxkbcommon-x11-0 \
    libxrender-dev \
    pkg-config \
    python3 \
    python3-dev \
    python3-pip \
    python3-setuptools \
    python3-venv \
    unzip \
    wget \
 && rm -rf /var/lib/apt/lists/*

RUN ln -sf /usr/bin/python3 /usr/bin/python

WORKDIR /opt
RUN git clone --depth 1 ${HY3D_REPO} /opt/Hunyuan3D-2.1 \
 && git -C /opt/Hunyuan3D-2.1 fetch --depth 1 origin ${HY3D_COMMIT} \
 && git -C /opt/Hunyuan3D-2.1 checkout ${HY3D_COMMIT}

WORKDIR /opt/Hunyuan3D-2.1

RUN python -m pip install --upgrade pip wheel setuptools \
 && pip install torch==2.5.1 torchvision==0.20.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cu124 \
 && grep -Ev '^(--extra-index-url|bpy==|deepspeed$|tb_nightly==)' requirements.txt > requirements.fly.txt \
 && pip install -r requirements.fly.txt

RUN cd hy3dpaint/custom_rasterizer \
 && export TORCH_CUDA_ARCH_LIST="8.6;8.9;9.0" \
 && export CUDA_NVCC_FLAGS="-allow-unsupported-compiler" \
 && pip install --no-build-isolation -e .

RUN cd hy3dpaint/DifferentiableRenderer && bash compile_mesh_painter.sh

RUN mkdir -p hy3dpaint/ckpt \
 && wget -q https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth -O hy3dpaint/ckpt/RealESRGAN_x4plus.pth

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV PORT=8081
EXPOSE 8081

ENTRYPOINT ["/entrypoint.sh"]
