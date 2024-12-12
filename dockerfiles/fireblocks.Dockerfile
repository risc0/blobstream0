FROM node:20
RUN npm install -g @fireblocks/fireblocks-json-rpc
ENTRYPOINT ["fireblocks-json-rpc"]