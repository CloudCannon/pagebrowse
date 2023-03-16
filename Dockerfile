FROM alpine

RUN apk --update upgrade && apk --no-cache add \
	gcc \
	curl \
	wget \
	make \
	bash \
	git \
	unzip \
	binutils \
	sudo \
	pkgconfig \
	webkit2gtk-5.0-dev \
	libappindicator \
	gtk4.0-dev \
	build-base \
	gtk+3.0-dev \
	libc6-compat \
	gst-plugins-good \
	xvfb \
	mesa-dri-gallium

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
	&& echo "cd /workspace" >> /root/.profile \
	&& echo "Xvfb :1 &" >> /root/.profile \
	&& echo ". ~/.profile" >> /root/.bashrc

ENV DISPLAY=:1

ENTRYPOINT [ "/bin/bash" ]