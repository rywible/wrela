FROM python:3.12-slim
WORKDIR /app
COPY apps/wrela-http-db-smoke/server.py /app/server.py
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1
ENV PORT=8080
EXPOSE 8080
CMD ["python", "/app/server.py"]
