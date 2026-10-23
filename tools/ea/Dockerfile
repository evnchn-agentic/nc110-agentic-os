# NIST SP 800-90B EntropyAssessment (reference ea_non_iid / ea_iid), pinned. Build:
#   docker build -t nc110-ea:87c104d tools/ea
FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends ca-certificates git g++ make \
    libbz2-dev libdivsufsort-dev libjsoncpp-dev libssl-dev libmpfr-dev libgmp-dev >/dev/null
RUN git clone -q https://github.com/usnistgov/SP800-90B_EntropyAssessment.git /ea \
 && cd /ea && git checkout -q 87c104d0ed4cbc96103e7b8b38d6f2c7e0a6b289 \
 && cd cpp && make -j4 non_iid iid >/dev/null 2>&1 && cp ea_non_iid ea_iid /usr/local/bin/
ENTRYPOINT []
