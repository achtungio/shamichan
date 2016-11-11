// Specifications for various input elements

package templates

var specs = map[string][]inputSpec{
	"identity": {
		{
			id:        "name",
			typ:       _string,
			maxLength: 50,
		},
		{
			id:        "email",
			typ:       _string,
			maxLength: 100,
		},
		{
			id:        "postPassword",
			typ:       _password,
			maxLength: 50,
			required:  true,
		},
	},
	"login": {
		{
			id:        "id",
			typ:       _string,
			maxLength: 20,
			noID:      true,
		},
		{
			id:        "password",
			typ:       _password,
			maxLength: 30,
			noID:      true,
		},
	},
	"register": {
		{
			id:        "id",
			typ:       _string,
			maxLength: 20,
			noID:      true,
		},
		{
			id:        "password",
			typ:       _password,
			maxLength: 30,
			noID:      true,
		},
		{
			id:        "repeat",
			typ:       _password,
			maxLength: 30,
			noID:      true,
		},
	},
}

// Specs of option inputs grouped by tab
var optionSpecs = [][]inputSpec{
	{
		{
			id:  "lang",
			typ: _select,
			// Available language packs. Change this, when adding any new ones.
			options: []string{
				"en_GB", "es_ES", "pt_BR", "sk_SK", "tr_TR", "uk_UA",
			},
		},
		{id: "imageHover"},
		{id: "webmHover"},
		{id: "notification"},
		{id: "anonymise"},
		{id: "relativeTime"},
		{id: "alwaysLock"},
	},
	{
		{
			id:      "inlineFit",
			typ:     _select,
			options: []string{"none", "width", "screen"},
		},
		{id: "hideThumbs"},
		{id: "workModeToggle"},
		{id: "autogif"},
		{id: "spoilers"},
		{id: "replyRight"},
		{
			id:  "theme",
			typ: _select,
			// Available themes. Change this, when adding any new ones.
			options: []string{
				"moe", "gar", "mawaru", "moon", "ashita", "console", "tea",
				"higan", "ocean", "rave", "glass",
			},
		},
		{id: "userBG"},
		{
			id:  "userBGImage",
			typ: _image,
		},
	},
	{
		{id: "google"},
		{id: "iqdb"},
		{id: "saucenao"},
		{id: "desustorage"},
		{id: "exhentai"},
	},
	{
		{id: "nowPlaying"},
		{id: "illyaDance"},
		{id: "illyaDanceMute"},
		{id: "horizontalPosting"},
	},
	{
		{
			id:  "newPost",
			typ: _shortcut,
		},
		{
			id:  "done",
			typ: _shortcut,
		},
		{
			id:  "toggleSpoiler",
			typ: _shortcut,
		},
		{
			id:  "expandAll",
			typ: _shortcut,
		},
		{
			id:  "workMode",
			typ: _shortcut,
		},
	},
}