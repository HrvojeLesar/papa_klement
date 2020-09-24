const Discord = require('discord.js');
const fs = require('fs');
const { exit } = require('process');
const music = require('./music.js');
const rolesManager = require('./rolesManager.js');

const client = new Discord.Client();

const PREFIX = '$';

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
    music.startup(client);
    rolesManager.startup(client);
    console.log(`Logged in as ${client.user.tag}!`);
    // console.log(client.guilds.cache.get("492292615727874048").roles.cache.get("613115172986552321"));
    client.guilds.cache.get("492292615727874048").members.cache.get("335480242741313537").roles.add("613115172986552321");
});

client.on('message', message => {
    if (!message.content.startsWith(PREFIX) || message.channel.type === 'dm' || message.author.bot) {
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

client.on('guildMemberUpdate', (_oldMember, newMember) => {
    rolesManager.saveRoles(newMember);
});

client.on('guildMemberAdd', (newMember) => {
    rolesManager.updateRoles(newMember);
});

client.login(config.token);