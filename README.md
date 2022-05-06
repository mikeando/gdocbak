# gdocbak

A very basic application for backing up google document. It downloads the latest
version of all google docs in your google drive.

You need to create a developer account and new application in the Google API pages.
And then set up an oath authentication flow.
You'll need to allow the `drive.readonly` and `drive.metadata.readonly` scopes - which are restricted but OK if you're
only using the application internally and with a limited number of users.
As part of this you will generate a client_id which you must store. I use `client_id.json`.
You will then need to put your application into production/published mode.

Then run the application like this. The first time it will ask you to authenticate, and give you 
a link to paste into a web-browser which will give you a token to paste back into the application.
But on subsequent runs you should not need to do this. (If you dont put your application into production mode
in the Google API pages then you will need to redo it every 2 weeks)

```
gdocbak --store=exports --client-settings=client_id.json --credentials=tokencache.json
```

This will download all your google documents to the exports directory, overwriting any existing files.
