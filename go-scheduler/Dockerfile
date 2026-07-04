FROM golang:1.24-alpine AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o openpipe-scheduler ./cmd/openpipe

FROM alpine:3.20
RUN apk --no-cache add ca-certificates
COPY --from=builder /app/openpipe-scheduler /usr/local/bin/
EXPOSE 8080 8081
CMD ["openpipe-scheduler"]
