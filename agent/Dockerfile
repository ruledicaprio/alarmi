FROM node:22-alpine

WORKDIR /app

# Install deps at build time (runs on internet-connected workstation)
COPY package.json ./
RUN npm install

COPY tsconfig.json ./
COPY src/ ./src/

# Build Mastra bundle
RUN npm run build

ENV NODE_ENV=production

# Mastra server listens on 4111 by default
EXPOSE 4111

CMD ["npm", "run", "start"]
