
FROM ubuntu:24.04

ARG TARGETARCH
ARG APP1=lxr_backup
ARG APP2=lxr_restore

COPY dist/${TARGETARCH}/${APP1} /usr/local/bin/${APP1}
COPY dist/${TARGETARCH}/${APP2} /usr/local/bin/${APP2}

RUN chmod +x /usr/local/bin/${APP1} /usr/local/bin/${APP2}

ENTRYPOINT ["/usr/local/bin/${APP1}"]
