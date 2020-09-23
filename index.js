const Discord = require('discord.js');
const fs = require('fs');
const ytdl = require('ytdl-core');
const { exit } = require('process');
const music = require('./music.js');

const client = new Discord.Client();
exports.client = client;

const PREFIX = '$';
exports.PREFIX = PREFIX;

let config;

let dispatcher = [];
let queue = [];

if (!fs.existsSync('./config.json')) {
    console.log('Missing config file!\nCreating new config.json!');
    fs.writeFileSync('./config.json', '{\n\t"token": "Please insert token"\n}');
} else {
    try {
        config = JSON.parse(fs.readFileSync('./config.json', 'utf-8'));
    } catch (err) {
        console.log(err);
        exit(0);
    }
}

client.on('ready', () => {
    console.log(`Logged in as ${client.user.tag}!`);
    music.startup(client);
});

client.on('message', message => {
    if (!message.content.startsWith(PREFIX) || message.channel.type === 'dm') {
        return;
    }

    let command = message.content.toLowerCase().split(' ');
    switch(command[0]) {
        case(PREFIX + 'play'): {
            music.play(message, command.slice(1).join(' '));
            break;
        }
        case(PREFIX + 'stop'): {
            music.stop(message);
            break;
        }
        case(PREFIX + 'skip'): {
            music.skip(message);
            break;
        }
        case(PREFIX + 'queue'): {
            music.queue(message);
            break;
        }

        default: { console.log("Invalid command " + command[0]); return; }
    }
});

client.login(config.token);

function startUp() {
    client.guilds.cache.forEach((guild) => {
        let guildId = guild.id;
        queue[guildId] = null;
        dispatcher[guildId] = null;
    });
}

function get_voice_channel(guild_id) {
    let channels = client.guilds.cache.get(guild_id).channels.cache.array();
    for (let i = 0; i < channels.length; i++) {
        if (channels[i].type === 'voice' && channels[i].members.size > 0) {
            if (channels[i].members.size == 1 && channels[i].members.array()[0].user.bot) {
                return undefined;
            }
            return channels[i];
        }
    }
    return undefined;
}